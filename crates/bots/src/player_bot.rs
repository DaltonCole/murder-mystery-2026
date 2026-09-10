//! One bot's whole lifecycle: join, then react to every pushed `View` by
//! taking one plausible action for whatever the current phase is asking
//! of it -- nominate, vote, attempt an open task, or exercise whichever
//! Phase 2/3 ability its own character currently has available -- mirroring
//! game-changer's `run_bot` (`examples/simulate_party.rs`), adapted for a
//! push-driven websocket instead of an HTTP polling loop: a real
//! `/play` connection gets a fresh `View` the instant anything relevant
//! changes (see `app::game_ws`'s broadcast mechanism), so a bot reacts to
//! each arrival instead of polling on a timer.
//!
//! Ability targeting is necessarily blind: `PlayerView` never reveals
//! another player's faction to anyone (see `view::view_for`'s "no ambient
//! god-view" design note), so a bot picks targets for `Convert`,
//! `DesignateSuccessor`, `TransferKingQueen`, etc. at random from the
//! active roster the same way a real player without X-ray vision would,
//! and just lets the server reject an ineligible guess -- the same
//! "rejected replies are silently ignored" convention `run` already uses
//! for `Nominate`/`CastBallot`. A field gated by `my_abilities` (an
//! explicit `_available` flag) is retried every view while still
//! available, since a successful use flips the flag itself and naturally
//! stops further attempts; a choice with no such flag (`DesignateSuccessor`,
//! `TransferKingQueen`) is capped at a few attempts instead, so a run of
//! bad guesses doesn't spam the connection for the rest of the game.

use crate::protocol::{ClientMsg, Conn, ConnError, ServerMsg};
use engine::{
    Ballot, Character, Command, DenouncementView, Faction, InfoQueryKind, PlayerId, PlayerStatus,
    PlayerView, RosterEntry, Round,
};
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// How many blind guesses `DesignateSuccessor`/`TransferKingQueen` will
/// make before giving up for the rest of the game -- neither has an
/// `AbilityStatus` flag to retry against (see the module doc comment), so
/// without a cap a run of bad guesses would resend on every single
/// broadcast for the rest of the game.
const MAX_BLIND_ATTEMPTS: u8 = 5;

pub struct PlayerBot {
    pub id: PlayerId,
    pub name: String,
    conn: Conn,
    rng: StdRng,
    /// Shared with every other bot and the `HostDriver` driving the
    /// Intermission draw -- see `HostDriver::draw_intermission_entrants`'s
    /// doc comment for why this can't just be read back off the wire.
    intermission_pool: Arc<Mutex<BTreeSet<PlayerId>>>,
    opted_into_intermission: bool,
    submitted_gallery_prediction: bool,
    /// Reset to `false` whenever no Denouncement is open -- `MedicProtect`
    /// is a standing choice that stays in effect for the whole round once
    /// declared, so re-sending it on every broadcast during the same open
    /// Denouncement would be pure noise.
    medic_declared_this_denouncement: bool,
    designate_successor_attempts: u8,
    transfer_king_queen_attempts: u8,
}

impl PlayerBot {
    /// Connects and joins as a brand new player, exactly like a real
    /// `/play` visitor typing their name and tapping Join.
    pub async fn join(
        url: &str,
        name: &str,
        seed: u64,
        intermission_pool: Arc<Mutex<BTreeSet<PlayerId>>>,
    ) -> Result<Self, ConnError> {
        let mut conn = Conn::connect(url).await?;
        let id = conn.join(name).await?;
        Ok(PlayerBot {
            id,
            name: name.to_string(),
            conn,
            rng: StdRng::seed_from_u64(seed),
            intermission_pool,
            opted_into_intermission: false,
            submitted_gallery_prediction: false,
            medic_declared_this_denouncement: false,
            designate_successor_attempts: 0,
            transfer_king_queen_attempts: 0,
        })
    }

    /// Runs until the connection closes, reacting to every pushed `View`.
    /// Returns normally on a clean close; never returns `Err` for a
    /// rejected action (a `Failed` reply is logged and ignored, matching
    /// how a real player's own client would just wait for the next
    /// legitimate opportunity rather than crashing) -- only a genuine
    /// transport failure propagates.
    pub async fn run(mut self) -> Result<(), ConnError> {
        loop {
            match self.conn.recv().await? {
                Some(ServerMsg::View(view)) => self.react(&view).await?,
                Some(ServerMsg::Failed { .. }) => {}
                Some(ServerMsg::Joined { .. }) => {}
                None => return Ok(()),
            }
        }
    }

    async fn react(&mut self, view: &PlayerView) -> Result<(), ConnError> {
        if let Some(phase) = &view.denouncement {
            self.react_to_denouncement(view, phase).await?;
        } else {
            self.medic_declared_this_denouncement = false;
        }
        for task in &view.open_tasks {
            if task.my_outcome.is_none() {
                if let Some(named) = pick_three_others(&view.roster, self.id, &mut self.rng) {
                    self.conn
                        .send(&ClientMsg::Do(Command::AttemptTask {
                            player: self.id,
                            task: task.id,
                            named,
                        }))
                        .await?;
                }
            }
        }
        self.react_to_abilities(view).await?;
        self.react_to_standing_choices(view).await?;
        Ok(())
    }

    async fn react_to_denouncement(
        &mut self,
        view: &PlayerView,
        phase: &DenouncementView,
    ) -> Result<(), ConnError> {
        match phase {
            DenouncementView::Nomination {
                i_have_acted: false,
            } => {
                if let Some(nominee) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                    self.conn
                        .send(&ClientMsg::Do(Command::Nominate {
                            voter: self.id,
                            nominee: nominee.id,
                        }))
                        .await?;
                }
                if view.own_character == Some(Character::Duelist)
                    && view.my_abilities.duelist_available == Some(true)
                {
                    if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng)
                    {
                        self.conn
                            .send(&ClientMsg::Do(Command::DuelistChallenge {
                                player: self.id,
                                target: target.id,
                            }))
                            .await?;
                    }
                }
            }
            DenouncementView::Discussion { .. } => {
                if view.own_character == Some(Character::Agitator)
                    && view.my_abilities.agitator_available == Some(true)
                {
                    if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng)
                    {
                        self.conn
                            .send(&ClientMsg::Do(Command::AgitatorRedirect {
                                player: self.id,
                                target: target.id,
                            }))
                            .await?;
                    }
                }
            }
            DenouncementView::Ballot {
                candidates,
                i_have_acted: false,
            }
            | DenouncementView::Runoff {
                candidates,
                i_have_acted: false,
            } => {
                // ~10% abstain, matching `sim`'s virtual-player strategy --
                // exercises the abstain path without letting it dominate.
                let ballot = match candidates.choose(&mut self.rng) {
                    Some(&candidate) if self.rng.random_range(0..10) != 0 => Ballot::For(candidate),
                    _ => Ballot::Abstain,
                };
                self.conn
                    .send(&ClientMsg::Do(Command::CastBallot {
                        voter: self.id,
                        ballot,
                    }))
                    .await?;
            }
            _ => {}
        }
        // The Grand Inquisitor's override applies to "whichever tally is
        // open" -- both Ballot and Runoff -- so it's checked once here
        // rather than duplicated in both match arms above.
        if matches!(
            phase,
            DenouncementView::Ballot { .. } | DenouncementView::Runoff { .. }
        ) && view.own_character == Some(Character::GrandInquisitor)
            && view.my_abilities.grand_inquisitor_available == Some(true)
        {
            self.conn
                .send(&ClientMsg::Do(Command::ActivateGrandInquisitor {
                    player: self.id,
                }))
                .await?;
        }
        Ok(())
    }

    /// Every Phase 2/3 ability gated by an explicit `AbilityStatus` field --
    /// safe to retry every view while the field still reports available,
    /// since a successful use flips it itself (see the module doc comment).
    async fn react_to_abilities(&mut self, view: &PlayerView) -> Result<(), ConnError> {
        let a = &view.my_abilities;

        if a.oracle_checks_available.is_some_and(|n| n > 0) {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.send(Command::UseOracle {
                    player: self.id,
                    target: target.id,
                })
                .await?;
            }
        }
        if a.almanac_available == Some(true) {
            self.send(Command::UseAlmanac { player: self.id }).await?;
        }
        if a.spymaster_available == Some(true) {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.send(Command::UseSpymaster {
                    player: self.id,
                    target: target.id,
                })
                .await?;
            }
        }
        if a.cult_leader_queries_available.is_some_and(|n| n > 0) {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                let kind = if self.rng.random_range(0..2) == 0 {
                    InfoQueryKind::IsTonAligned
                } else {
                    InfoQueryKind::IsTheLeader
                };
                self.send(Command::CultLeaderQuery {
                    player: self.id,
                    target: target.id,
                    kind,
                })
                .await?;
            }
        }
        if a.recruitment_slots_available.is_some_and(|n| n > 0) {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.send(Command::Convert {
                    converter: self.id,
                    target: target.id,
                })
                .await?;
            }
        }
        if a.deceiver_armed == Some(false) && a.deceiver_falsify_used == Some(false) {
            self.send(Command::SetDeceiverArmed {
                player: self.id,
                armed: true,
            })
            .await?;
        }
        if a.priest_protects_available.is_some_and(|n| n > 0) {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.send(Command::PriestProtect {
                    player: self.id,
                    target: target.id,
                })
                .await?;
            }
        }
        if a.medic_available == Some(true) && !self.medic_declared_this_denouncement {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.send(Command::MedicProtect {
                    player: self.id,
                    target: target.id,
                })
                .await?;
                self.medic_declared_this_denouncement = true;
            }
        }
        if a.bartender_available == Some(true) {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                let lands = self.rng.random_range(0..2) == 0;
                self.send(Command::BartenderTarget {
                    player: self.id,
                    target: target.id,
                    lands,
                })
                .await?;
            }
        }
        if a.potion_maker_available == Some(true) {
            self.send(Command::ActivatePotionImmunity { player: self.id })
                .await?;
        }
        if a.double_vote_available == Some(true) {
            self.send(Command::ActivateDoubleVote { player: self.id })
                .await?;
        }
        if a.vote_shield_available == Some(true) {
            self.send(Command::ArmVoteShield { player: self.id })
                .await?;
        }
        Ok(())
    }

    /// The remaining Phase 2/3 commands: standing choices with no
    /// `AbilityStatus` flag of their own (capped, see `MAX_BLIND_ATTEMPTS`),
    /// and the Intermission/Gallery mechanics gated on the viewer's own
    /// public status rather than their character.
    async fn react_to_standing_choices(&mut self, view: &PlayerView) -> Result<(), ConnError> {
        if view.own_character == Some(Character::RevolutionaryLeader)
            && self.designate_successor_attempts < MAX_BLIND_ATTEMPTS
        {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.designate_successor_attempts += 1;
                self.send(Command::DesignateSuccessor {
                    leader: self.id,
                    successor: target.id,
                })
                .await?;
            }
        }
        if view.own_character == Some(Character::KingQueen)
            && view.current_round < Round::Five
            && self.transfer_king_queen_attempts < MAX_BLIND_ATTEMPTS
        {
            if let Some(target) = active_others(&view.roster, self.id).choose(&mut self.rng) {
                self.transfer_king_queen_attempts += 1;
                self.send(Command::TransferKingQueen {
                    new_holder: target.id,
                })
                .await?;
            }
        }

        if !self.opted_into_intermission
            && my_status(&view.roster, self.id) == Some(PlayerStatus::Active)
        {
            self.opted_into_intermission = true;
            self.intermission_pool.lock().unwrap().insert(self.id);
            self.send(Command::OptIntoIntermission { player: self.id })
                .await?;
        }

        if !self.submitted_gallery_prediction
            && my_status(&view.roster, self.id) == Some(PlayerStatus::CastOut)
            && view.current_round == Round::Finale
            && view.denouncement.is_some()
        {
            self.submitted_gallery_prediction = true;
            let prediction = if self.rng.random_range(0..2) == 0 {
                let candidate = view
                    .roster
                    .choose(&mut self.rng)
                    .map(|r| r.id)
                    .unwrap_or(self.id);
                engine::GalleryPrediction::CastOutIs(candidate)
            } else {
                let faction = [Faction::Ton, Faction::Uprising, Faction::Cult]
                    .choose(&mut self.rng)
                    .copied()
                    .unwrap();
                engine::GalleryPrediction::FactionWins(faction)
            };
            self.send(Command::SubmitGalleryPrediction {
                player: self.id,
                prediction,
            })
            .await?;
        }
        Ok(())
    }

    async fn send(&mut self, cmd: Command) -> Result<(), ConnError> {
        self.conn.send(&ClientMsg::Do(cmd)).await
    }
}

fn my_status(roster: &[RosterEntry], me: PlayerId) -> Option<PlayerStatus> {
    roster.iter().find(|r| r.id == me).map(|r| r.status)
}

fn active_others(roster: &[RosterEntry], me: PlayerId) -> Vec<&RosterEntry> {
    roster
        .iter()
        .filter(|r| r.status == PlayerStatus::Active && r.id != me)
        .collect()
}

fn pick_three_others(
    roster: &[RosterEntry],
    me: PlayerId,
    rng: &mut StdRng,
) -> Option<[PlayerId; 3]> {
    let mut others: Vec<PlayerId> = active_others(roster, me)
        .into_iter()
        .map(|r| r.id)
        .collect();
    if others.len() < 3 {
        return None;
    }
    use rand::seq::SliceRandom;
    others.shuffle(rng);
    Some([others[0], others[1], others[2]])
}
