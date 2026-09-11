//! Drives the host side of a game over the real websocket protocol:
//! setup (faction/title assignment, matching `sim`'s same rules.md §2
//! ratios), Round 1's tasks, and each Denouncement's open/wait/close
//! cycle through to the Finale. Not itself a "bot" -- the host is the
//! authoritative game-runner, not a simulated player, so this issues
//! direct commands rather than reacting to its own view (mirroring
//! game-changer's `TestServer` helper methods like `admin_move`/
//! `admin_force_start`, which the test calls directly rather than
//! modeling the admin as a bot thread).
//!
//! Every step here that runs *after* bots have joined and might be acting
//! concurrently uses `Conn::do_cmd_until` with a predicate on the Host's
//! own view, not the simpler "assume the next reply is mine" approach --
//! see that method's doc comment for why the latter is unsound once N
//! bots are simultaneously nominating/voting/attempting tasks.
//!
//! The raffle-commit sequence inside `setup_game` (`AssignCharacter` for
//! every winner, `AssignFaction` for the leftovers, `CloseRaffle`,
//! `FinalizeSetup`) uses the simpler `do_cmd_sequential` instead -- but,
//! critically, over a *fresh* connection opened just for that sequence,
//! not `self.conn`. Every on-time bot fires its own `SubmitInterestLevel`
//! the instant it joins (see `PlayerBot::react`), and `self.conn` has been
//! subscribed to Host broadcasts since `connect()` -- so by the time
//! `setup_game` runs, it can already be sitting on a backlog of those
//! bots' broadcast pushes. `do_cmd_sequential` on a connection with a
//! backlog is exactly the unsound case its own doc comment warns about:
//! confirmed by reproducing "a late arrival's own `AddPlayer` gets applied
//! before `CloseRaffle` does" this way, under real load, at 30 players. A
//! brand-new connection has no such backlog (broadcasts are pushed to
//! already-subscribed connections in real time, never replayed to a new
//! subscriber), so `do_cmd_sequential` is genuinely safe on it regardless
//! of what any bot is doing concurrently.

use crate::protocol::{Conn, ConnError};
use engine::{
    raffle_priority, raffle_winners, ticket_count, ticket_slots, Character, Command,
    ContestCategory, DenouncementView, Faction, PlayerId, PlayerView, Round, TaskTier, Viewer,
};
use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct HostDriver {
    conn: Conn,
    url: String,
}

/// The four raffle-won titles, for a caller to sanity-check the setup
/// raffle's outcome without re-deriving it. `None` only in a pathologically
/// tiny on-time pool (fewer than 4 players) that couldn't fill every major
/// role -- `setup_game` still runs to completion in that case, matching how
/// `Command::FinalizeSetup` already treats "not enough named-role
/// candidates" as a normal small-game outcome rather than an error. Late
/// arrivals are deliberately absent here -- see `Command::AddPlayer`'s doc
/// comment: they're auto-`Faction::Servant`'d the instant they join, so
/// there's no setup-time list of them for this struct to hand back.
pub struct Roles {
    pub king_queen: Option<PlayerId>,
    pub prince_princess: Option<PlayerId>,
    pub revolutionary_leader: Option<PlayerId>,
    pub cult_leader: Option<PlayerId>,
}

impl HostDriver {
    pub async fn connect(url: &str) -> Result<Self, ConnError> {
        let mut conn = Conn::connect(url).await?;
        conn.watch(Viewer::Host).await?;
        Ok(HostDriver {
            conn,
            url: url.to_string(),
        })
    }

    /// Polls the host's own view until the roster reaches `count`
    /// players (every bot has successfully joined) or `timeout` elapses.
    /// Safe to just inspect whatever view arrives here (direct reply or
    /// broadcast alike) -- unlike a write-command's confirmation, "has the
    /// roster grown to N yet" is a plain read of current truth either way.
    pub async fn wait_for_roster(
        &mut self,
        count: usize,
        timeout: Duration,
    ) -> Result<Vec<PlayerId>, ConnError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let view = self.conn.watch(Viewer::Host).await?;
            if view.roster.len() >= count {
                return Ok(view.roster.iter().map(|r| r.id).collect());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ConnError::Timeout(
                    "roster reaching the expected player count",
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Polls the host's own view until every one of `count` on-time
    /// players has submitted an interest level (rules.md §1), or `timeout`
    /// elapses -- the setup raffle can't run before then.
    pub async fn wait_for_interest_levels(
        &mut self,
        count: usize,
        timeout: Duration,
    ) -> Result<(), ConnError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let view = self.conn.watch(Viewer::Host).await?;
            if view.interest_levels.len() >= count {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ConnError::Timeout(
                    "every on-time player submitting an interest level",
                ));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Runs rules.md §1's weighted setup raffle over `player_ids` (the
    /// on-time roster -- anyone who joins later is a separate "late
    /// arrival" the caller handles outside this method, see
    /// `Command::AddPlayer`'s doc comment) and finalizes setup:
    ///
    /// 1. Waits for every on-time player's `SubmitInterestLevel`.
    /// 2. Computes each player's tickets (`engine::ticket_count`), expands
    ///    them into a flat ticket list, and shuffles it with `seed` -- the
    ///    one genuinely random step, which has to happen here rather than
    ///    in `engine` (see `raffle`'s module doc comment on "randomness at
    ///    the boundary"). Players with zero tickets go into a separately-
    ///    shuffled low-interest fallback pool instead, for rules.md's
    ///    "unless the pool is otherwise underfilled" clause.
    /// 3. Awards every named role (`engine::raffle_winners`) via
    ///    `AssignCharacter`, which auto-assigns the matching faction too
    ///    (see its own doc comment) -- rules.md §1 assigns roles *before*
    ///    factions specifically so raffle interest, not a manual pick,
    ///    decides who can become Cult Leader.
    /// 4. Splits everyone who didn't win a role across Ton/Uprising
    ///    (~60/40, rules.md §1) via `AssignFaction`.
    /// 5. Closes the raffle (`Command::CloseRaffle`) and finalizes setup.
    ///
    /// Steps 3-5 use `do_cmd_sequential` over a fresh connection opened
    /// just for them, not `self.conn` -- see the module doc comment for
    /// why `self.conn` (already subscribed to Host broadcasts since it can
    /// have been watching since before every on-time bot's own
    /// interest-level submission) isn't safe for this anymore.
    pub async fn setup_game(
        &mut self,
        player_ids: &[PlayerId],
        seed: u64,
    ) -> Result<Roles, ConnError> {
        self.wait_for_interest_levels(player_ids.len(), Duration::from_secs(15))
            .await?;
        let view = self.conn.watch(Viewer::Host).await?;
        let submitted: BTreeMap<PlayerId, u8> = view.interest_levels.iter().copied().collect();

        let mut rng = StdRng::seed_from_u64(seed);
        let mut tickets = BTreeMap::new();
        let mut low_interest = Vec::new();
        for &id in player_ids {
            let level = submitted.get(&id).copied().unwrap_or(0);
            let count = ticket_count(level);
            if count > 0 {
                tickets.insert(id, count);
            } else {
                low_interest.push(id);
            }
        }
        let mut slots = ticket_slots(&tickets);
        slots.shuffle(&mut rng);
        low_interest.shuffle(&mut rng);
        let priority = raffle_priority(&slots, &low_interest);
        let winners = raffle_winners(&priority);

        // See the module doc comment: a brand-new connection here (not
        // `self.conn`) has no broadcast backlog to misattribute a reply
        // from, unlike the long-lived, already-subscribed Host connection.
        let mut setup_conn = Conn::connect(&self.url).await?;
        setup_conn.watch(Viewer::Host).await?;

        for &(character, player) in &winners {
            setup_conn
                .do_cmd_sequential(Command::AssignCharacter { player, character })
                .await?;
        }
        let title_winner = |wanted: Character| {
            winners
                .iter()
                .find(|&&(character, _)| character == wanted)
                .map(|&(_, player)| player)
        };
        let roles = Roles {
            king_queen: title_winner(Character::KingQueen),
            prince_princess: title_winner(Character::PrincePrincess),
            revolutionary_leader: title_winner(Character::RevolutionaryLeader),
            cult_leader: title_winner(Character::CultLeader),
        };

        let won_a_role: BTreeSet<PlayerId> = winners.iter().map(|&(_, player)| player).collect();
        let mut remaining: Vec<PlayerId> = player_ids
            .iter()
            .copied()
            .filter(|id| !won_a_role.contains(id))
            .collect();
        remaining.shuffle(&mut rng);
        let ton_count = (remaining.len() as f64 * 0.6).round() as usize;
        let (ton, uprising) = remaining.split_at(ton_count);
        for (group, faction) in [(ton, Faction::Ton), (uprising, Faction::Uprising)] {
            for &id in group {
                setup_conn
                    .do_cmd_sequential(Command::AssignFaction {
                        player: id,
                        faction,
                    })
                    .await?;
            }
        }

        setup_conn.do_cmd_sequential(Command::CloseRaffle).await?;
        setup_conn.do_cmd_sequential(Command::FinalizeSetup).await?;
        Ok(roles)
    }

    /// Round 1 (rules.md §4): pushes 2 fixed tasks, waits `phase_wait` for
    /// bots to attempt them, then closes submissions. `open_tasks`'s
    /// length is directly observable in the Host's own view (unlike
    /// faction/character), so both steps use `do_cmd_until`.
    pub async fn run_round_one_tasks(
        &mut self,
        everyone: &[PlayerId],
        seed: u64,
        phase_wait: Duration,
    ) -> Result<(), ConnError> {
        let mut rng = StdRng::seed_from_u64(seed);
        for (pushed, (prompt, tier)) in [
            ("Talk to someone wearing a mask", TaskTier::Easy),
            ("Talk to someone who loves to dance", TaskTier::Medium),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, pair)| (i + 1, pair))
        {
            let qualifier = everyone.choose(&mut rng).copied();
            self.conn
                .do_cmd_until(
                    Command::PushTask {
                        prompt: prompt.into(),
                        tier,
                        qualifying_players: qualifier.into_iter().collect(),
                    },
                    |v| v.open_tasks.len() >= pushed,
                    Duration::from_secs(10),
                    "task count increasing after PushTask",
                )
                .await?;
        }
        tokio::time::sleep(phase_wait).await;
        self.conn
            .do_cmd_until(
                Command::CloseTasks,
                |v| v.open_tasks.is_empty(),
                Duration::from_secs(10),
                "open_tasks clearing after CloseTasks",
            )
            .await?;
        Ok(())
    }

    /// Rounds 3 & 5's task phase (rules.md §4: "Task phase, easy/medium/hard
    /// tiers live"), pushing one bio-derived task per tier from
    /// `PlayerView::task_candidates` (Host-only; see that field's doc
    /// comment) -- unlike Round 1's fixed prompts, these come from whatever
    /// the room's own submitted bios actually contain. A tier with no
    /// candidates yet (e.g. nobody filled in that category) is silently
    /// skipped rather than erroring, same as pushing zero fixed tasks would
    /// be harmless.
    pub async fn run_bio_driven_tasks(&mut self, phase_wait: Duration) -> Result<(), ConnError> {
        let view = self.view().await?;
        let mut pushed = 0usize;
        for (tier, candidates) in view.task_candidates {
            let Some(candidate) = candidates.into_iter().next() else {
                continue;
            };
            pushed += 1;
            self.conn
                .do_cmd_until(
                    Command::PushTask {
                        prompt: candidate.prompt,
                        tier,
                        qualifying_players: candidate.qualifying_players.into_iter().collect(),
                    },
                    |v| v.open_tasks.len() >= pushed,
                    Duration::from_secs(10),
                    "task count increasing after PushTask",
                )
                .await?;
        }
        tokio::time::sleep(phase_wait).await;
        self.conn
            .do_cmd_until(
                Command::CloseTasks,
                |v| v.open_tasks.is_empty(),
                Duration::from_secs(10),
                "open_tasks clearing after CloseTasks",
            )
            .await?;
        Ok(())
    }

    pub async fn advance_round_to(&mut self, expected: Round) -> Result<PlayerView, ConnError> {
        self.conn
            .do_cmd_until(
                Command::AdvanceRound,
                |v| v.current_round == expected,
                Duration::from_secs(10),
                "current_round reaching the expected round",
            )
            .await
    }

    /// Runs one full Denouncement (rules.md §5): Nomination -> Discussion
    /// -> Ballot -> optional Runoff -> resolution, waiting `phase_wait`
    /// between opening a phase and closing it so bots have a chance to
    /// act -- the same real-time-driven shape a live host running a
    /// countdown timer would follow, just on a much shorter clock. Every
    /// step confirms its own observable phase transition rather than
    /// trusting reply order, since bots are actively nominating/voting
    /// concurrently by the time this runs.
    pub async fn run_denouncement(
        &mut self,
        phase_wait: Duration,
    ) -> Result<PlayerView, ConnError> {
        self.conn
            .do_cmd_until(
                Command::OpenDenouncement,
                |v| matches!(v.denouncement, Some(DenouncementView::Nomination { .. })),
                Duration::from_secs(10),
                "Nomination phase opening",
            )
            .await?;
        tokio::time::sleep(phase_wait).await;
        self.conn
            .do_cmd_until(
                Command::CloseNomination,
                |v| matches!(v.denouncement, Some(DenouncementView::Discussion { .. })),
                Duration::from_secs(10),
                "Discussion phase opening",
            )
            .await?;
        self.conn
            .do_cmd_until(
                Command::OpenBallot,
                |v| matches!(v.denouncement, Some(DenouncementView::Ballot { .. })),
                Duration::from_secs(10),
                "Ballot phase opening",
            )
            .await?;
        tokio::time::sleep(phase_wait).await;
        let view = self
            .conn
            .do_cmd_until(
                Command::CloseBallot {
                    fallback_replacement: None,
                },
                |v| !matches!(v.denouncement, Some(DenouncementView::Ballot { .. })),
                Duration::from_secs(10),
                "Ballot phase closing (to Runoff or resolved)",
            )
            .await?;

        if matches!(view.denouncement, Some(DenouncementView::Runoff { .. })) {
            tokio::time::sleep(phase_wait).await;
            return self
                .conn
                .do_cmd_until(
                    Command::CloseRunoff {
                        fallback_replacement: None,
                    },
                    |v| v.denouncement.is_none(),
                    Duration::from_secs(10),
                    "Runoff phase closing",
                )
                .await;
        }
        Ok(view)
    }

    pub async fn view(&mut self) -> Result<PlayerView, ConnError> {
        self.conn.watch(Viewer::Host).await
    }

    /// Records all three contest categories for `round` (rules.md §4:
    /// Strength, Creativity, Intelligence) -- `round` must be `Two` or
    /// `Four`. The activities themselves are still undesigned app content
    /// (see `contest.rs`'s doc comment), so this just picks an arbitrary
    /// winner per category; only the wire path is under test here, not
    /// which faction the coin flip favors.
    pub async fn record_contest_results_for_round(
        &mut self,
        round: Round,
        seed: u64,
    ) -> Result<(), ConnError> {
        let mut rng = StdRng::seed_from_u64(seed);
        for category in [
            ContestCategory::Strength,
            ContestCategory::Creativity,
            ContestCategory::Intelligence,
        ] {
            self.conn
                .do_cmd_until(
                    Command::RecordContestResult {
                        round,
                        category,
                        ton_won: rng.random_range(0..2) == 0,
                    },
                    |v| {
                        v.contest_results
                            .iter()
                            .any(|&((r, c), _)| r == round && c == category)
                    },
                    Duration::from_secs(10),
                    "contest_results reflecting the recorded category",
                )
                .await?;
        }
        Ok(())
    }

    /// Draws up to 5 Intermission entrants (rules.md §4) from `opt_in_pool`
    /// -- a plain in-process set the bots themselves add to as they opt in
    /// (see `PlayerBot::react`), standing in for whatever real-world
    /// mechanism (a physical raffle box, players raising a hand) a live
    /// host would actually use. This is deliberately NOT read from the
    /// wire: `view_for` never exposes the opt-in pool to the Host, by
    /// design -- see `GameState::opted_into_intermission`'s doc comment on
    /// why it stays private even from the Host, unlike the drawn entrants
    /// themselves once this runs.
    pub async fn draw_intermission_entrants(
        &mut self,
        opt_in_pool: &Arc<Mutex<BTreeSet<PlayerId>>>,
        seed: u64,
    ) -> Result<(), ConnError> {
        let view = self.view().await?;
        let active: BTreeSet<PlayerId> = view
            .roster
            .iter()
            .filter(|r| r.status == engine::PlayerStatus::Active)
            .map(|r| r.id)
            .collect();
        let mut candidates: Vec<PlayerId> = opt_in_pool
            .lock()
            .unwrap()
            .iter()
            .filter(|id| active.contains(id))
            .copied()
            .collect();
        let mut rng = StdRng::seed_from_u64(seed);
        candidates.shuffle(&mut rng);
        candidates.truncate(5);

        self.conn
            .do_cmd_until(
                Command::DrawIntermissionEntrants {
                    selected: candidates,
                },
                |v| v.intermission_entrants.is_some(),
                Duration::from_secs(10),
                "intermission_entrants being drawn",
            )
            .await?;
        Ok(())
    }

    /// Awards one Servant leaderboard point to each of `players` -- the
    /// same "objective fact the host records" shape as contest results.
    /// Uses `do_cmd_until` (not `do_cmd_sequential`) since this always runs
    /// while bots are concurrently active, unlike setup.
    pub async fn award_servant_points(
        &mut self,
        players: impl IntoIterator<Item = PlayerId>,
    ) -> Result<(), ConnError> {
        for player in players {
            self.conn
                .do_cmd_until(
                    Command::AwardServantPoints { player, points: 1 },
                    |v| v.servant_leaderboard.iter().any(|&(id, _)| id == player),
                    Duration::from_secs(10),
                    "servant_leaderboard reflecting the award",
                )
                .await?;
        }
        Ok(())
    }

    /// Scores every submitted Gallery prediction against the real outcome
    /// (rules.md §7) -- `newly_cast_out` is whoever the Finale's own
    /// Denouncement just resolved (the caller diffs the roster's CastOut
    /// set before/after `run_denouncement` at the Finale; see
    /// `run_full_automated_game`). Reads the actual winner from the Host's
    /// own view (`PlayerView::winner`, `GameState::winner_for_host`'s doc
    /// comment explains why only the Host ever sees this). A completed game
    /// where nobody's condition is met is still possible in one narrow
    /// residual case even after Dalton's follow-up ruling closed the common
    /// one (see `win_condition::evaluate`'s doc comment) --
    /// `ResolveGalleryPredictions` has no slot for that, so this harness
    /// falls back to an arbitrary `Faction::Ton` in that rare case purely
    /// to keep exercising the command's wire path, not as a claim that Ton
    /// actually won. Uses `do_cmd_until` against `PlayerView::gallery_resolved`
    /// rather than `do_cmd_sequential`, since bots can still be concurrently
    /// active at this point in the game (they aren't `.abort()`'d until
    /// after this returns).
    pub async fn resolve_gallery_predictions(
        &mut self,
        newly_cast_out: Vec<PlayerId>,
    ) -> Result<PlayerView, ConnError> {
        let view = self.view().await?;
        let actual_winner = view.winner.unwrap_or(Faction::Ton);
        self.conn
            .do_cmd_until(
                Command::ResolveGalleryPredictions {
                    actual_cast_out: newly_cast_out,
                    actual_winner,
                },
                |v| v.gallery_resolved,
                Duration::from_secs(10),
                "gallery_resolved flipping true",
            )
            .await
    }
}
