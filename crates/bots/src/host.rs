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
//! bots are simultaneously nominating/voting/attempting tasks. Setup
//! (before anyone has anything to react to yet) uses the simpler
//! `do_cmd_sequential` since no concurrent activity is possible there.

use crate::protocol::{Conn, ConnError};
use engine::{
    Character, Command, DenouncementView, Faction, PlayerId, PlayerView, Round, TaskTier, Viewer,
};
use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::SeedableRng;
use std::time::Duration;

pub struct HostDriver {
    conn: Conn,
}

/// The four titled players a setup pass installed, plus everyone else --
/// enough for a caller to sanity-check the split without re-deriving it.
pub struct Roles {
    pub king_queen: PlayerId,
    pub prince_princess: PlayerId,
    pub revolutionary_leader: PlayerId,
    pub cult_leader: PlayerId,
}

impl HostDriver {
    pub async fn connect(url: &str) -> Result<Self, ConnError> {
        let mut conn = Conn::connect(url).await?;
        conn.watch(Viewer::Host).await?;
        Ok(HostDriver { conn })
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

    /// Assigns factions (rules.md §2: Servants ~10%, Cult seeded with just
    /// the Cult Leader, remaining ~60/40 Ton/Uprising) and the four titles
    /// to an already-joined roster, then finalizes setup -- the same
    /// ratios `sim::setup_game` uses, just issued as real commands instead
    /// of direct `apply_command` calls. Uses `do_cmd_sequential`: nothing
    /// else is happening yet (bots have nothing to react to before any
    /// faction/character/Denouncement/task exists), so there's no
    /// concurrent broadcast traffic to misattribute a reply from.
    pub async fn setup_game(
        &mut self,
        player_ids: &[PlayerId],
        seed: u64,
    ) -> Result<Roles, ConnError> {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut ids = player_ids.to_vec();
        ids.shuffle(&mut rng);

        let n = ids.len();
        let servant_count = (n as f64 * 0.10).round() as usize;
        let cult_count = 1;
        let remaining = n.saturating_sub(servant_count + cult_count);
        let ton_count = ((remaining as f64) * 0.6).round() as usize;
        let uprising_count = remaining - ton_count;

        let mut cursor = 0;
        let servants = &ids[cursor..cursor + servant_count];
        cursor += servant_count;
        let cult = &ids[cursor..cursor + cult_count];
        cursor += cult_count;
        let ton = &ids[cursor..cursor + ton_count];
        cursor += ton_count;
        let uprising = &ids[cursor..cursor + uprising_count];

        for (group, faction) in [
            (servants, Faction::Servant),
            (cult, Faction::Cult),
            (ton, Faction::Ton),
            (uprising, Faction::Uprising),
        ] {
            for &id in group {
                self.conn
                    .do_cmd_sequential(Command::AssignFaction {
                        player: id,
                        faction,
                    })
                    .await?;
            }
        }

        let roles = Roles {
            king_queen: ton[0],
            prince_princess: ton[1],
            revolutionary_leader: uprising[0],
            cult_leader: cult[0],
        };
        for (player, character) in [
            (roles.king_queen, Character::KingQueen),
            (roles.prince_princess, Character::PrincePrincess),
            (roles.revolutionary_leader, Character::RevolutionaryLeader),
            (roles.cult_leader, Character::CultLeader),
        ] {
            self.conn
                .do_cmd_sequential(Command::AssignCharacter { player, character })
                .await?;
        }

        self.conn.do_cmd_sequential(Command::FinalizeSetup).await?;
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
}
