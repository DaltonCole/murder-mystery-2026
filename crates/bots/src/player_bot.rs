//! One bot's whole lifecycle: join, then react to every pushed `View` by
//! taking one plausible action for whatever the current phase is asking
//! of it -- nominate, vote, or attempt an open task -- mirroring
//! game-changer's `run_bot` (`examples/simulate_party.rs`), adapted for a
//! push-driven websocket instead of an HTTP polling loop: a real
//! `/play` connection gets a fresh `View` the instant anything relevant
//! changes (see `app::game_ws`'s broadcast mechanism), so a bot reacts to
//! each arrival instead of polling on a timer.

use crate::protocol::{ClientMsg, Conn, ConnError, ServerMsg};
use engine::{Ballot, Command, DenouncementView, PlayerId, PlayerStatus, PlayerView, RosterEntry};
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{RngExt, SeedableRng};

pub struct PlayerBot {
    pub id: PlayerId,
    pub name: String,
    conn: Conn,
    rng: StdRng,
}

impl PlayerBot {
    /// Connects and joins as a brand new player, exactly like a real
    /// `/play` visitor typing their name and tapping Join.
    pub async fn join(url: &str, name: &str, seed: u64) -> Result<Self, ConnError> {
        let mut conn = Conn::connect(url).await?;
        let id = conn.join(name).await?;
        Ok(PlayerBot {
            id,
            name: name.to_string(),
            conn,
            rng: StdRng::seed_from_u64(seed),
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
        Ok(())
    }
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
