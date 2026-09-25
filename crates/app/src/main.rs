//! The three role-based routes from the implementation plan (`/`=play,
//! `/host`, `/display`), talking to the single game server over one typed
//! websocket connection (see `game_server` for the server-side state and
//! `ClientMsg`/`ServerMsg` below for the wire protocol).
//!
//! Phase 1 scope: these are genuinely basic shells -- functional enough for
//! a real LAN playtest, not the final visual design (that's Phase 4). See
//! `/home/drc/.claude/plans/piped-crunching-lighthouse.md`.
//!
//! FIXED, formerly a KNOWN GAP: a security review (2026-09-25) found
//! `game_ws` originally accepted any `ClientMsg` from any connection with
//! no identity check at all -- `ViewPlayer(Some(id))` could silently dump
//! any player's entire private view, `Watch`/`Do(Command)` could read or
//! act as any player or the Host with nothing tying a claim to who
//! actually sent it, and every host-only command was reachable from a
//! plain, unauthenticated connection. This is now enforced server-side,
//! not just by the Host UI's own choice not to render controls: every
//! connection tracks `own_player_id` (set once, only by that connection's
//! own successful `Join`) and `is_host_authed` (set only by a successful
//! `HostLogin`), and every `ClientMsg`/`Command` is checked against them
//! -- see `command_actor`/`command_authorized` and the `Watch`/`ViewPlayer`
//! checks in `game_ws` below for exactly what's enforced. Still not a full
//! session layer: `own_player_id` lives only in this one connection's own
//! in-memory state, set once from `Join`'s reply -- there's no token a
//! player's browser could use to reclaim the same identity after a dropped
//! connection or reload (`Join` always mints a brand-new `PlayerId`, never
//! re-attaches to an existing one). See the SECOND KNOWN GAP below, which
//! is the same underlying "no reconnect story" limitation.
//!
//! SECOND KNOWN GAP: no reconnect story. Every route's `use_websocket` call
//! uses a plain `WebSocketOptions::new()`, not
//! `.with_automatic_reconnect()`; once a connection drops (a WiFi hiccup,
//! laptop sleep, a `dx serve` restart), the only recovery is a manual page
//! reload. `Host` now at least shows a visible "reload now" banner the
//! moment this happens (a reliability review found it previously gave zero
//! indication at all, worst for this route specifically since the Host is
//! the one person running the whole live event) -- `Play`/`Display` still
//! don't. Deliberately not wiring up automatic reconnect in this pass:
//! doing so safely also requires re-verifying the receive-loop's error
//! handling (see the `Err(_) => break` comments in `Play`/`Host`/`Display`
//! below) against real reconnect behavior in an actual browser, which this
//! environment can't do -- see the session summary for why. Over a
//! multi-hour live event on venue WiFi, this is worth fixing for real
//! before Phase 5, with a real browser available to verify it.

#[cfg(feature = "server")]
mod game_server;

use dioxus::fullstack::{use_websocket, WebSocketOptions, Websocket};
use dioxus::prelude::*;
use engine::{
    pascal_case, AbilityStatus, Ballot, Bio, Character, Command, ContestCategory, DenouncementView,
    DomainEvent, Faction, GalleryPrediction, InfoCheckAnswer, InfoCheckDelivery, InfoQueryKind,
    PlayerId, PlayerReveal, PlayerStatus, PlayerView, RosterEntry, Round, TaskTier, TaskView,
    Viewer, WhistledownPost, MIN_CATEGORY_ENTRIES,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[route("/")]
    Play {},
    #[route("/host")]
    Host {},
    #[route("/display")]
    Display {},
}

fn main() {
    // A reliability review found `check_host_password` is silently wide
    // open (accepts any password, including "") whenever `HOST_PASSWORD`
    // isn't set -- a reasonable default for local dev (see that function's
    // own doc comment), but there was previously nothing to stop that from
    // being the state at a real event too, if it's just forgotten. This is
    // the one moment guaranteed to reach a terminal the host is actually
    // looking at, right before `make run`'s own "give players this URL"
    // line -- printed only on the server binary, never the WASM client.
    #[cfg(feature = "server")]
    if std::env::var("HOST_PASSWORD").is_err() {
        eprintln!(
            "WARNING: HOST_PASSWORD is not set -- the Host console at /host will accept ANY passphrase, including a blank one. Set HOST_PASSWORD before a real event."
        );
    }
    dioxus::launch(App);
}

// System-font-stack theming only -- see `assets/main.css`'s own doc
// comment for why this app never reaches for a Google Fonts (or any
// other external) stylesheet.
const MAIN_CSS: Asset = asset!("/assets/main.css");
// `assets/manifest.json` embeds its own icon as an inline base64 data URI
// rather than referencing a separate `asset!()`-served file -- `dx` gives
// every `asset!()` file a content-hashed filename but doesn't rewrite
// references *inside* another static file to match (the same reason
// game-changer's own `main.rs` inlines its `@font-face` rule instead of
// putting it in its CSS file), so a plain `"icons": [{"src": "/icon.svg"}]`
// would silently 404 the moment the hash changes on a rebuild. The data
// URI sidesteps that entirely: the icon lives inside the one file that
// references it, with nothing else to go stale.
const MANIFEST: Asset = asset!("/assets/manifest.json");
const ICON: Asset = asset!("/assets/icon.svg");

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "manifest", href: MANIFEST }
        document::Link { rel: "icon", href: ICON, r#type: "image/svg+xml" }
        document::Meta { name: "theme-color", content: "#4a1620" }
        // Without this, mobile browsers render the page at a virtual
        // desktop-width layout viewport and scale it down -- every
        // control on this phone-first app would show up tiny and
        // require pinch-zoom. A review pass found this missing entirely.
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1" }
        // Deliberately no Play/Host/Display navigation menu -- Dalton's own
        // explicit instruction: `/`, `/host`, and `/display` are known URLs
        // only he hands out (to players, to himself, to the projector
        // laptop), not something every visitor should be able to discover
        // and switch between from a shared menu. An earlier version had one
        // here specifically to fix "no way to discover the other routes
        // exist" -- since deliberately removed for the opposite reason.
        Router::<Route> {}
    }
}

// --- Wire protocol -------------------------------------------------------
//
// Everything any route needs to do -- join, watch, or act -- goes over this
// one typed websocket, the same mechanism Phase 0 spiked and proved works
// end to end.

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ClientMsg {
    /// `/play` only: registers a new player and starts watching them.
    Join { name: String },
    /// `/host` and `/display`: start watching as that role. `/play` never
    /// sends this directly with an arbitrary id -- see the module doc
    /// comment's KNOWN GAP note.
    Watch(Viewer),
    /// Any other game command.
    Do(Command),
    /// `/host` only: runs rules.md §1's weighted setup raffle over the
    /// current roster and finalizes setup. Not a plain `Command` -- unlike
    /// everything else here, this needs a real RNG (see
    /// `game_server::run_raffle`'s doc comment on "randomness at the
    /// boundary"), which only exists server-side.
    RunRaffle,
    /// `/host` only: pushes `game_server::LOCATION_TASKS[index]` as a real
    /// task. Not a plain `Command` either -- the code that task carries
    /// only exists server-side (see `game_server::push_location_task`'s
    /// doc comment), so the Host browser can only ever refer to a template
    /// by index, never construct the `PushTask` itself.
    PushLocationTask { index: usize },
    /// `/host` only: draws the Intermission entrants server-side. Not a
    /// plain `Command { selected }` either -- unlike a real player's own
    /// `Command::OptIntoIntermission`, the Host browser has no legitimate
    /// way to know *who* opted in to pass as `selected` in the first
    /// place: `view_for` deliberately never reveals the opt-in pool to
    /// `Viewer::Host` (see `game_server::draw_intermission_entrants`'s doc
    /// comment). The server reads `GameState` directly instead.
    DrawIntermissionEntrants,
    /// `/host` only: (re)starts the shared round/phase timer at this many
    /// seconds. Not a `Command` -- see `game_server::start_timer`'s doc
    /// comment on why wall-clock time is an app-layer concept here, never
    /// part of `GameState`.
    StartTimer { seconds: u32 },
    /// `/host` only: adds seconds to the running timer (or starts one if
    /// none is running). See `game_server::add_timer_seconds`.
    AddTimerSeconds { seconds: u32 },
    /// `/host` only: clears the timer entirely.
    ClearTimer,
    /// `/host` only: attempts to log in with this passphrase. See
    /// `game_server::check_host_password`'s doc comment.
    HostLogin { password: String },
    /// `/play` only, the King/Queen's crown-transfer ability: not a plain
    /// `Command` -- rules.md gives the King/Queen no say in who receives
    /// the crown ("a random remaining Ton player"), so there's no target
    /// for the client to supply at all. See
    /// `game_server::transfer_king_queen_randomly`'s doc comment.
    TransferKingQueen { player: PlayerId },
    /// `/host` only: watch a specific player's own `PlayerView` alongside
    /// the Host's normal `Viewer::Host` view, for the Host console's
    /// read-only "view a player's page" panel -- lets Dalton help a
    /// confused player without walking over and touching their phone.
    /// `Some(id)` starts/switches the subscription; `None` clears it. Not
    /// folded into the existing `Watch` message -- a connection already
    /// watching as `Viewer::Host` needs both views at once, not a
    /// replacement for either. Deliberately read-only: the reply
    /// (`ServerMsg::ViewedPlayer`) carries the same `PlayerView` that
    /// player would see themselves, and nothing in this protocol lets a
    /// Host connection issue a `Command` *as* that player -- see
    /// `PlayerPageReadOnly`'s doc comment.
    ViewPlayer(Option<PlayerId>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ServerMsg {
    Joined {
        player: PlayerId,
    },
    View(PlayerView),
    Failed {
        error: String,
    },
    /// `/host` only, sent once right after a `Watch(Viewer::Host)`
    /// succeeds: the safe (tier, prompt) subset of every pre-authored
    /// location task template, for the Host's "push" picker -- see
    /// `game_server::location_task_templates`'s doc comment for why the
    /// code itself never rides along.
    LocationTaskTemplates(Vec<(usize, TaskTier, String)>),
    /// Sent to every viewer kind (this is rules.md's "shared timer") on
    /// every `Watch` and every broadcast tick -- seconds remaining, or
    /// `None` if no timer is currently running. Deliberately not part of
    /// `PlayerView`/`ServerMsg::View` -- see `game_server::GameTimer`'s
    /// doc comment on why wall-clock time stays out of the engine's
    /// deterministic state entirely.
    Timer(Option<i64>),
    /// `/host` only, direct reply to `ClientMsg::HostLogin`: whether the
    /// submitted passphrase was correct.
    HostLoginResult {
        ok: bool,
    },
    /// `/host` only, reply to and kept live after `ClientMsg::ViewPlayer`:
    /// the named player's own `PlayerView`, or `None` once cleared. Kept
    /// out of the ordinary `View` variant (which always carries the
    /// requesting connection's own `Viewer::Host` view) so a Host
    /// connection can hold both at once.
    ViewedPlayer(Option<PlayerView>),
}

/// The `PlayerId` a `Command` acts as, if it names one at all --
/// `Command::CastBallot { voter, .. }` returns `voter`, `Command::Convert {
/// converter, .. }` returns `converter`, and so on for every variant that
/// carries the acting player's own identity. The ~18 purely administrative
/// variants (`AdvanceRound`, `OpenDenouncement`, `CastOut`, `PushTask`,
/// ...) return `None` -- these are things the Host clicks a button for,
/// never a player's own self-service action, regardless of what `player`
/// fields might appear elsewhere in their payload (e.g. `CastOut`'s
/// `player` names who's being removed, not who's asking).
///
/// A security review (2026-09-25) found `game_ws` enforced none of this at
/// all: any raw websocket connection could send any `Command` claiming to
/// act as any player, with nothing tying the claim to who actually sent
/// it. `command_authorized` below is what fixes that, using this function
/// to decide "does this command's claimed actor match who this connection
/// actually joined as." Deliberately exhaustive with no wildcard arm: a
/// future `Command` variant added to the engine without a decision made
/// here is a compile error, not a silent new hole.
///
/// `#[cfg(feature = "server")]`: only `game_ws`'s real (server-side) body
/// calls this -- the `web`/WASM build compiles a network-calling stub for
/// `game_ws` instead (see the `#[get(...)]` server-function macro), so
/// without this gate the `web` build would see both functions below as
/// dead code.
#[cfg(feature = "server")]
fn command_actor(cmd: &Command) -> Option<PlayerId> {
    match cmd {
        Command::AddPlayer { .. }
        | Command::CloseRaffle
        | Command::AssignFaction { .. }
        | Command::AssignCharacter { .. }
        | Command::FinalizeSetup
        | Command::CastOut { .. }
        | Command::AdvanceRound
        | Command::OpenDenouncement
        | Command::CloseNomination
        | Command::OpenBallot
        | Command::CloseBallot { .. }
        | Command::CloseRunoff { .. }
        | Command::PushTask { .. }
        | Command::CloseTasks
        | Command::RecordContestResult { .. }
        | Command::DrawIntermissionEntrants { .. }
        | Command::AwardServantPoints { .. }
        | Command::ResolveGalleryPredictions { .. } => None,

        Command::SubmitInterestLevel { player, .. }
        | Command::SubmitBio { player, .. }
        | Command::TransferKingQueen { player, .. }
        | Command::AttemptTask { player, .. }
        | Command::AttemptLocationTask { player, .. }
        | Command::UseOracle { player, .. }
        | Command::UseAlmanac { player }
        | Command::UseSpymaster { player, .. }
        | Command::CultLeaderQuery { player, .. }
        | Command::SetDeceiverArmed { player, .. }
        | Command::PriestProtect { player, .. }
        | Command::MedicProtect { player, .. }
        | Command::BartenderTarget { player, .. }
        | Command::ActivatePotionImmunity { player, .. }
        | Command::ActivateDoubleVote { player }
        | Command::DuelistChallenge { player, .. }
        | Command::AgitatorRedirect { player, .. }
        | Command::ActivateGrandInquisitor { player }
        | Command::ArmVoteShield { player }
        | Command::OptIntoIntermission { player }
        | Command::SubmitGalleryPrediction { player, .. } => Some(*player),

        Command::Convert { converter, .. } => Some(*converter),
        Command::DesignateSuccessor { leader, .. } => Some(*leader),
        Command::Nominate { voter, .. } => Some(*voter),
        Command::CastBallot { voter, .. } => Some(*voter),
    }
}

/// Whether this connection may submit `cmd`: either it's logged in as
/// Host (trusted for everything), or `cmd` names exactly the player this
/// connection itself joined as -- including `Convert`, whose only UI is
/// now the Cult Leader's own `/play` ability panel (Dalton's own explicit
/// instruction: the Host shouldn't have this power, only the Cult Leader
/// does). Every purely administrative command (`command_actor` returns
/// `None`) requires the Host login regardless, since there's no player
/// identity for it to ever match.
#[cfg(feature = "server")]
fn command_authorized(
    cmd: &Command,
    is_host_authed: bool,
    own_player_id: Option<PlayerId>,
) -> bool {
    is_host_authed || command_actor(cmd).is_some_and(|actor| own_player_id == Some(actor))
}

#[get("/api/ws")]
async fn game_ws(options: WebSocketOptions) -> Result<Websocket<ClientMsg, ServerMsg>> {
    Ok(options.on_upgrade(move |mut socket| async move {
        let mut viewer: Option<Viewer> = None;
        // `/host` only -- see `ClientMsg::ViewPlayer`'s doc comment. Kept
        // separate from `viewer` above (which for a Host connection stays
        // `Some(Viewer::Host)`) so watching a player's page never replaces
        // the Host's own view.
        let mut viewing_player: Option<PlayerId> = None;
        // The player identity this specific connection actually joined
        // as (set once, by `ClientMsg::Join`'s own success) -- the source
        // of truth `command_authorized`/`Watch` check a claimed actor
        // against. Never set by anything the client merely *claims*.
        let mut own_player_id: Option<PlayerId> = None;
        // Set once `ClientMsg::HostLogin` succeeds. See `command_authorized`
        // and this file's module doc comment for what this now actually
        // gates -- before this field existed, `HostLogin`'s reply was
        // purely informational and gated nothing server-side at all.
        let mut is_host_authed = false;
        let mut changed = game_server::subscribe();

        // `game_server::apply` broadcasts on every successful mutation, this
        // connection's own included. The `Join`/`Do` branches below already
        // send a direct, up-to-date `View` in response to the command that
        // *caused* the change -- without draining the echo of that same
        // broadcast here, the next `select!` iteration would immediately
        // fire the `changed.recv()` arm too and send a second, redundant
        // `View` for a change this connection already knows about.
        fn drain_self_echo(changed: &mut tokio::sync::broadcast::Receiver<()>) {
            while changed.try_recv().is_ok() {}
        }

        // Shared reply shape for every Host-mutation `ClientMsg` arm below
        // (`Do`, `RunRaffle`, `PushLocationTask`, `DrawIntermissionEntrants`):
        // on success, drain this connection's own broadcast echo and send a
        // fresh view if it's watching as someone; on failure, relay the
        // error as `ServerMsg::Failed`. Only the called mutation differs per
        // arm -- a macro (not a function) so it can reach `socket`/
        // `changed`/`viewer` directly, sidestepping the need to spell out
        // this connection's concrete websocket type.
        macro_rules! respond {
            ($result:expr) => {
                match $result {
                    Ok(_) => {
                        drain_self_echo(&mut changed);
                        if let Some(v) = viewer {
                            socket.send(ServerMsg::View(game_server::view(v))).await.is_ok()
                        } else {
                            true
                        }
                    }
                    Err(e) => socket
                        .send(ServerMsg::Failed { error: e.to_string() })
                        .await
                        .is_ok(),
                }
            };
        }

        // Sends an authorization-failure `Failed` reply -- the same shape
        // `respond!`'s `Err` arm uses, just without a real `GameError` to
        // format (this rejection never reaches `apply_command` at all).
        macro_rules! reject {
            ($msg:expr) => {
                socket
                    .send(ServerMsg::Failed {
                        error: $msg.to_string(),
                    })
                    .await
                    .is_ok()
            };
        }

        loop {
            tokio::select! {
                incoming = socket.recv() => {
                    let Ok(msg) = incoming else { break };
                    let sent_ok = match msg {
                        ClientMsg::Join { name } => {
                            match game_server::apply(Command::AddPlayer { name }) {
                                Ok(events) => {
                                    let Some(DomainEvent::PlayerAdded { id, .. }) =
                                        events.into_iter().next()
                                    else {
                                        continue;
                                    };
                                    viewer = Some(Viewer::Player(id));
                                    own_player_id = Some(id);
                                    drain_self_echo(&mut changed);
                                    socket.send(ServerMsg::Joined { player: id }).await.is_ok()
                                        && socket
                                            .send(ServerMsg::View(game_server::view(Viewer::Player(id))))
                                            .await
                                            .is_ok()
                                        && socket
                                            .send(ServerMsg::Timer(game_server::timer_remaining_secs()))
                                            .await
                                            .is_ok()
                                }
                                Err(e) => socket
                                    .send(ServerMsg::Failed { error: e.to_string() })
                                    .await
                                    .is_ok(),
                            }
                        }
                        ClientMsg::Watch(v) => {
                            // A security review found this previously accepted
                            // ANY `Viewer`, letting a crafted client read any
                            // player's private data or the Host's aggregate
                            // view just by claiming it. Now: a player may only
                            // watch as the id they themselves joined as, Host
                            // requires a successful login, and Display (never
                            // privileged -- see `view_for`) stays open to all.
                            let allowed = match v {
                                Viewer::Player(id) => own_player_id == Some(id),
                                Viewer::Host => is_host_authed,
                                Viewer::Display => true,
                            };
                            if !allowed {
                                reject!("not authorized to watch as this viewer")
                            } else {
                                viewer = Some(v);
                                let sent_view = socket.send(ServerMsg::View(game_server::view(v))).await.is_ok();
                                // The Host's location-task picker needs the safe
                                // template metadata once, right after it starts
                                // watching -- see `ServerMsg::LocationTaskTemplates`'s
                                // doc comment for why this doesn't ride along
                                // inside `PlayerView` itself.
                                let sent_templates = if matches!(v, Viewer::Host) {
                                    socket
                                        .send(ServerMsg::LocationTaskTemplates(
                                            game_server::location_task_templates(),
                                        ))
                                        .await
                                        .is_ok()
                                } else {
                                    true
                                };
                                let sent_timer = socket
                                    .send(ServerMsg::Timer(game_server::timer_remaining_secs()))
                                    .await
                                    .is_ok();
                                sent_view && sent_templates && sent_timer
                            }
                        }
                        ClientMsg::Do(cmd) => {
                            if command_authorized(&cmd, is_host_authed, own_player_id) {
                                respond!(game_server::apply(cmd))
                            } else {
                                reject!("not authorized to perform this action")
                            }
                        }
                        ClientMsg::RunRaffle => {
                            if is_host_authed {
                                respond!(game_server::run_raffle())
                            } else {
                                reject!("host login required")
                            }
                        }
                        ClientMsg::PushLocationTask { index } => {
                            if is_host_authed {
                                respond!(game_server::push_location_task(index))
                            } else {
                                reject!("host login required")
                            }
                        }
                        ClientMsg::DrawIntermissionEntrants => {
                            if is_host_authed {
                                respond!(game_server::draw_intermission_entrants())
                            } else {
                                reject!("host login required")
                            }
                        }
                        ClientMsg::StartTimer { seconds } => {
                            if !is_host_authed {
                                reject!("host login required")
                            } else {
                                game_server::start_timer(seconds);
                                drain_self_echo(&mut changed);
                                socket
                                    .send(ServerMsg::Timer(game_server::timer_remaining_secs()))
                                    .await
                                    .is_ok()
                            }
                        }
                        ClientMsg::AddTimerSeconds { seconds } => {
                            if !is_host_authed {
                                reject!("host login required")
                            } else {
                                game_server::add_timer_seconds(seconds);
                                drain_self_echo(&mut changed);
                                socket
                                    .send(ServerMsg::Timer(game_server::timer_remaining_secs()))
                                    .await
                                    .is_ok()
                            }
                        }
                        ClientMsg::ClearTimer => {
                            if !is_host_authed {
                                reject!("host login required")
                            } else {
                                game_server::clear_timer();
                                drain_self_echo(&mut changed);
                                socket
                                    .send(ServerMsg::Timer(game_server::timer_remaining_secs()))
                                    .await
                                    .is_ok()
                            }
                        }
                        ClientMsg::HostLogin { password } => {
                            is_host_authed = game_server::check_host_password(&password);
                            socket
                                .send(ServerMsg::HostLoginResult { ok: is_host_authed })
                                .await
                                .is_ok()
                        }
                        ClientMsg::TransferKingQueen { player } => {
                            if is_host_authed || own_player_id == Some(player) {
                                respond!(game_server::transfer_king_queen_randomly(player))
                            } else {
                                reject!("not authorized to transfer this crown")
                            }
                        }
                        ClientMsg::ViewPlayer(target) => {
                            if !is_host_authed {
                                reject!("host login required")
                            } else {
                                viewing_player = target;
                                socket
                                    .send(ServerMsg::ViewedPlayer(
                                        target.map(|id| game_server::view(Viewer::Player(id))),
                                    ))
                                    .await
                                    .is_ok()
                            }
                        }
                    };
                    if !sent_ok {
                        break;
                    }
                }
                _ = changed.recv() => {
                    if let Some(v) = viewer {
                        if socket.send(ServerMsg::View(game_server::view(v))).await.is_err() {
                            break;
                        }
                    }
                    if let Some(id) = viewing_player {
                        if socket
                            .send(ServerMsg::ViewedPlayer(Some(game_server::view(Viewer::Player(id)))))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    // Keeps the round timer display live for everyone even
                    // when the only thing that happened was the ticker's
                    // own once-a-second nudge (`game_server::ensure_ticker_running`)
                    // -- harmless to send on every other kind of change too.
                    if socket
                        .send(ServerMsg::Timer(game_server::timer_remaining_secs()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    }))
}

// --- /play -----------------------------------------------------------------

/// The three screens a player sees once they've finished Character
/// Creation (submitted a bio) -- a simplified pre-game version once a bio
/// exists but `raffle_closed` is still false (Character shows the
/// submitted bio read-only, Round just says "waiting for the game to
/// start," Game is the same as always), then the real thing once
/// `raffle_closed` flips true. Before a bio's even submitted, `Play` is a
/// single Character Creation form instead, with no tabs at all. Grouped by
/// subject rather than by game phase so the tab a player wants is always
/// the same one, round after round: "who am I" (Character), "what do I
/// need to do right now" (Round), and "what's happening across the whole
/// game" (Game).
#[derive(Debug, Clone, Copy, PartialEq)]
enum PlayTab {
    Character,
    Round,
    Game,
}

#[component]
fn Play() -> Element {
    let mut view = use_signal(|| None::<PlayerView>);
    let mut my_id = use_signal(|| None::<PlayerId>);
    let mut error = use_signal(|| None::<String>);
    let mut name_draft = use_signal(String::new);
    let mut timer = use_signal(|| None::<i64>);
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        loop {
            match socket.recv().await {
                Ok(ServerMsg::Joined { player }) => my_id.set(Some(player)),
                // Deliberately NOT clearing `error` here. This connection
                // gets a fresh `View` on every successful command from
                // *any* connected player, not just this one's own (one
                // shared broadcast channel, see `game_server::apply`'s doc
                // comment) -- a review found that clearing the error banner
                // on every such push meant someone else's unrelated action
                // could silently wipe a just-shown rejection before this
                // player finished reading it. `send_cmd` below clears it
                // instead, only on this player's own next action.
                Ok(ServerMsg::View(v)) => view.set(Some(v)),
                Ok(ServerMsg::Failed { error: e }) => error.set(Some(e)),
                // `/play` never watches as `Viewer::Host`, and never sends
                // `HostLogin`/`ViewPlayer` either -- see
                // `ServerMsg::LocationTaskTemplates`'s doc comment for why
                // these still have to be parseable.
                Ok(
                    ServerMsg::LocationTaskTemplates(_)
                    | ServerMsg::HostLoginResult { .. }
                    | ServerMsg::ViewedPlayer(_),
                ) => {}
                Ok(ServerMsg::Timer(remaining)) => timer.set(remaining),
                Err(_) => break,
            }
        }
    });

    let mut send_cmd = move |cmd: Command| {
        let socket = socket;
        error.set(None);
        spawn(async move {
            let _ = socket.send(ClientMsg::Do(cmd)).await;
        });
    };

    // Not a plain `Command` -- the King/Queen's crown transfer picks its
    // own random target server-side (rules.md: "a random remaining Ton
    // player"), so there's no target for this player to supply at all.
    let mut transfer_king_queen = move |player: PlayerId| {
        let socket = socket;
        error.set(None);
        spawn(async move {
            let _ = socket.send(ClientMsg::TransferKingQueen { player }).await;
        });
    };

    let mut gallery_pick = use_signal(|| None::<u32>);
    let mut gallery_faction_pick = use_signal(|| None::<Faction>);
    // rules.md's Round 1 has Dalton live-demo a "press and hold" reveal
    // before everyone privately reveals their own character -- a review
    // pass found this ritual had no matching UI at all: the moment a
    // faction/character was assigned, it just appeared as static text
    // with nothing to actually press or hold. Once tripped, stays revealed
    // for the rest of the game -- there's no reason to re-hide it.
    let mut revealed = use_signal(|| false);
    // Which of the three tabs is showing -- meaningful once a bio's been
    // submitted (see `PlayTab`'s own doc comment for the pre-game vs.
    // post-game version of each); before that, `Play` shows a single
    // Character Creation form instead, with no tabs at all.
    let mut active_tab = use_signal(|| PlayTab::Character);

    let mut do_join = move || {
        let name = name_draft.peek().trim().to_string();
        if name.is_empty() {
            return;
        }
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::Join { name }).await;
        });
        name_draft.set(String::new());
    };

    let Some(id) = my_id() else {
        return rsx! {
            h1 { "Murder Mystery 2026" }
            if let Some(e) = error() {
                p { class: "error-text", "{e}" }
            }
            input {
                placeholder: "Your name",
                value: "{name_draft}",
                oninput: move |e| name_draft.set(e.value()),
                onkeydown: move |e: Event<KeyboardData>| {
                    if e.key() == Key::Enter {
                        do_join();
                    }
                },
            }
            button { onclick: move |_| do_join(), "Join the game" }
        };
    };

    let Some(v) = view() else {
        return rsx! { p { "Connecting..." } };
    };

    // Late arrivals (rules.md: "late arrivals become Servants") -- the
    // only players who ever reach the tabbed view below without going
    // through Character Creation first, since `Faction::Servant` is only
    // ever assigned after the raffle's closed. See `PlayTab`'s doc
    // comment for why this and `is_cast_out` are computed once here
    // rather than re-checked inline at each of their several call sites
    // below (a review found the same `roster.iter().any(...)` check
    // repeated three times over).
    let is_servant = v.own_faction == Some(Faction::Servant);
    let is_cast_out = v
        .roster
        .iter()
        .any(|r| r.id == id && r.status == PlayerStatus::CastOut);

    // Shared by both the pre-game tabbed screen and the real post-game one
    // -- neither the tab buttons themselves nor which one's active depend
    // on whether the game's actually started, only `active_tab` (a plain
    // `Copy` `Signal`), so this is safe to build once and splice into
    // whichever of the two mutually-exclusive branches below actually
    // renders, the same "build once, used in exactly one branch" shape
    // `AbilityPanel`'s own `target_picker` already uses.
    let tab_nav = rsx! {
        div {
            class: "tab-nav",
            button {
                class: if active_tab() == PlayTab::Character { "tab-active" },
                onclick: move |_| active_tab.set(PlayTab::Character),
                "Character",
            }
            button {
                class: if active_tab() == PlayTab::Round { "tab-active" },
                onclick: move |_| active_tab.set(PlayTab::Round),
                "Round",
            }
            button {
                class: if active_tab() == PlayTab::Game { "tab-active" },
                onclick: move |_| active_tab.set(PlayTab::Game),
                "Game",
            }
        }
    };

    rsx! {
        h1 { "Murder Mystery 2026" }
        if let Some(e) = error() {
            p { class: "error-text", "{e}" }
        }
        if v.raffle_closed {
            p { "Round: {v.current_round:?}" }
            TimerDisplay { remaining_secs: timer() }
            if let Some(faction) = v.own_faction {
                if faction == Faction::Unassigned {
                    p { "Waiting for setup to finish..." }
                } else if revealed() {
                    p {
                        "Your faction: {faction:?}"
                        if let Some(c) = v.own_character {
                            " -- {character_label(c)}"
                        }
                    }
                } else {
                    div {
                        class: "reveal-gate",
                        p { "Your character is ready. Press and hold below to reveal it -- just to you." }
                        button {
                            onclick: move |_| revealed.set(true),
                            "Press and hold to reveal"
                        }
                    }
                }
            }
            FinaleCastOutReveal { reveal: v.finale_cast_out_reveal.clone(), heading_level: 4u8 }
            {tab_nav}
            if active_tab() == PlayTab::Character {
                if is_servant {
                    div {
                        h3 { "You're a Servant" }
                        p { "{faction_flavor(Faction::Servant)}" }
                        p { "You joined after the game started (rules.md: late arrivals become Servants), so there's no character sheet or ability panel for you. You can still nominate, vote, and attempt tasks like everyone else." }
                    }
                } else {
                    if let Some(faction) = v.own_faction {
                        if faction != Faction::Unassigned {
                            p { "{faction_flavor(faction)}" }
                        }
                    }
                    if let Some(c) = v.own_character {
                        p { "{character_flavor(c)}" }
                    }
                    if let Some(king_queen) = v.known_king_queen {
                        p { "You now know the King/Queen: {names(&[king_queen], &v.roster)}." }
                    }
                    if let Some(leader) = v.revealed_leader {
                        p { "You now know the Revolutionary Leader: {names(&[leader], &v.roster)}." }
                    }
                    if !v.my_confidants.is_empty() {
                        p { "These people now know you're the Revolutionary Leader: {names(&v.my_confidants, &v.roster)}." }
                    }
                    if let Some(message) = &v.martyrdom_message {
                        p { class: "martyrdom-message",
                            "{message}"
                            br {}
                            em { "(private -- only you can see this)" }
                        }
                    }
                    BioForm {
                        my_id: id,
                        own_bio: v.own_bio.clone(),
                        locked: true,
                        on_command: send_cmd,
                    }
                    AbilityPanel {
                        my_id: id,
                        own_character: v.own_character,
                        abilities: v.my_abilities.clone(),
                        my_info_checks: v.my_info_checks.clone(),
                        fellow_cultists: v.fellow_cultists.clone(),
                        known_uprising_members: v.known_uprising_members.clone(),
                        roster: v.roster.clone(),
                        on_command: send_cmd,
                        on_transfer_king_queen: move |()| transfer_king_queen(id),
                    }
                }
            }
            if active_tab() == PlayTab::Round {
                if v.i_am_drunk {
                    p { class: "error-text", "You're drunk this round -- you can't nominate or vote." }
                }
                for task in v.open_tasks.clone() {
                    if task.is_location_task {
                        LocationTaskAttemptForm {
                            key: "{task.id.0}",
                            my_id: id,
                            task,
                            on_command: send_cmd,
                        }
                    } else {
                        TaskAttemptForm {
                            key: "{task.id.0}",
                            my_id: id,
                            task,
                            roster: v.roster.clone(),
                            on_command: send_cmd,
                        }
                    }
                }
                if is_cast_out {
                    p { "You've been Cast Out -- you're spectating the rest of the Denouncement." }
                } else {
                    DenouncementPanel {
                        my_id: id,
                        denouncement: v.denouncement.clone(),
                        roster: v.roster.clone(),
                        on_command: send_cmd,
                    }
                }
                if is_cast_out {
                    div {
                        h4 { "The Gallery" }
                        if v.current_round != Round::Finale {
                            p { "The Gallery opens once the Finale begins." }
                        } else {
                            p { "Predict who gets Cast Out, or which faction wins -- scored against the Servant leaderboard once the Finale resolves." }
                            select {
                                onchange: move |e| gallery_pick.set(e.value().parse().ok()),
                                option { value: "", "-- who gets Cast Out? --" }
                                for r in v.roster.iter().filter(|r| r.status == PlayerStatus::Active) {
                                    option { value: "{r.id.0}", "{r.name}" }
                                }
                            }
                            button {
                                disabled: gallery_pick().is_none(),
                                onclick: move |_| {
                                    let Some(t) = gallery_pick() else { return };
                                    send_cmd(Command::SubmitGalleryPrediction {
                                        player: id,
                                        prediction: GalleryPrediction::CastOutIs(PlayerId(t)),
                                    });
                                },
                                "Predict this Cast-Out",
                            }
                            select {
                                onchange: move |e| {
                                    gallery_faction_pick.set(match e.value().as_str() {
                                        "Ton" => Some(Faction::Ton),
                                        "Uprising" => Some(Faction::Uprising),
                                        "Cult" => Some(Faction::Cult),
                                        _ => None,
                                    });
                                },
                                option { value: "", "-- who wins? --" }
                                option { value: "Ton", "Ton" }
                                option { value: "Uprising", "Uprising" }
                                option { value: "Cult", "Cult" }
                            }
                            button {
                                disabled: gallery_faction_pick().is_none(),
                                onclick: move |_| {
                                    let Some(f) = gallery_faction_pick() else { return };
                                    send_cmd(Command::SubmitGalleryPrediction {
                                        player: id,
                                        prediction: GalleryPrediction::FactionWins(f),
                                    });
                                },
                                "Predict this winner",
                            }
                        }
                    }
                }
            }
            if active_tab() == PlayTab::Game {
                div {
                    h4 { "Intermission" }
                    if let Some(entrants) = &v.intermission_entrants {
                        p { "Entrants: {names(entrants, &v.roster)}" }
                    } else if v.i_opted_into_intermission {
                        p { "You're in the pool. Entrants haven't been drawn yet." }
                    } else if is_cast_out || is_servant {
                        p { "Cast-Out players and Servants aren't eligible for the Intermission lottery." }
                    } else {
                        button {
                            onclick: move |_| send_cmd(Command::OptIntoIntermission { player: id }),
                            "Opt into the Intermission lottery",
                        }
                    }
                }
                if !v.servant_leaderboard.is_empty() {
                    div {
                        h4 { "Servant leaderboard" }
                        ul {
                            for (pid , points) in v.servant_leaderboard.clone() {
                                li { key: "{pid.0}", "{names(&[pid], &v.roster)}: {points}" }
                            }
                        }
                    }
                }
                WhistledownPosts { posts: v.whistledown.clone(), heading_level: 4u8 }
                RosterList { roster: v.roster.clone() }
            }
        } else if v.own_bio.is_some() {
            // Character Creation is done (a bio's been submitted) but the
            // game hasn't started yet -- Dalton's own explicit instruction:
            // no "Round: One"/"waiting for setup" noise here, just a
            // simplified version of the same three-tab shape the real
            // post-game screen uses, so the transition into the game once
            // setup finalizes doesn't relocate anything a player's already
            // gotten used to finding.
            {tab_nav}
            if active_tab() == PlayTab::Character {
                if let Some(level) = v.own_interest_level {
                    p { "Your interest level: {level}" }
                }
                BioForm {
                    my_id: id,
                    own_bio: v.own_bio.clone(),
                    locked: true,
                    on_command: send_cmd,
                }
            }
            if active_tab() == PlayTab::Round {
                p { "Waiting for the game to start." }
            }
            if active_tab() == PlayTab::Game {
                div {
                    h4 { "Intermission" }
                    if let Some(entrants) = &v.intermission_entrants {
                        p { "Entrants: {names(entrants, &v.roster)}" }
                    } else if v.i_opted_into_intermission {
                        p { "You're in the pool. Entrants haven't been drawn yet." }
                    } else {
                        button {
                            onclick: move |_| send_cmd(Command::OptIntoIntermission { player: id }),
                            "Opt into the Intermission lottery",
                        }
                    }
                }
                if !v.servant_leaderboard.is_empty() {
                    div {
                        h4 { "Servant leaderboard" }
                        ul {
                            for (pid , points) in v.servant_leaderboard.clone() {
                                li { key: "{pid.0}", "{names(&[pid], &v.roster)}: {points}" }
                            }
                        }
                    }
                }
                WhistledownPosts { posts: v.whistledown.clone(), heading_level: 4u8 }
                RosterList { roster: v.roster.clone() }
            }
        } else {
            // rules.md §1's "Character creation" -- a new player's very
            // first task, before there's any round/game state worth
            // showing. Servants never reach this branch (they only ever
            // join once `raffle_closed` is already true, which routes them
            // into the first branch above instead), so there's no "how
            // involved do you want to be" or "create your character"
            // prompt for them at all -- both are meaningless once the
            // raffle they'd feed into has already run.
            div {
                h2 { "Character Creation" }
                p { "Welcome! The game hasn't started yet -- rate how involved you'd like to be, then create your character below." }
                InterestLevelForm {
                    my_id: id,
                    own_interest_level: v.own_interest_level,
                    on_command: send_cmd,
                }
                BioForm {
                    my_id: id,
                    own_bio: v.own_bio.clone(),
                    locked: false,
                    on_command: send_cmd,
                }
            }
        }
    }
}

/// rules.md §1's signup interest rating -- only shown during Character
/// Creation, before the setup raffle has closed (`Play`'s caller gates
/// this on `!v.raffle_closed`, not `own_character.is_none()` -- a review
/// found the old character-based gate never actually hid this from a
/// Servant, since a late arrival's `own_character` stays `None` forever,
/// not just until the raffle runs). A standing choice like `SubmitBio` --
/// resubmitting silently replaces (see `Command::SubmitInterestLevel`'s
/// doc comment), so this doesn't need a separate "already submitted, lock
/// it in" state.
#[component]
fn InterestLevelForm(
    my_id: PlayerId,
    own_interest_level: Option<u8>,
    on_command: EventHandler<Command>,
) -> Element {
    let mut level = use_signal(|| own_interest_level.unwrap_or(5));
    rsx! {
        div {
            h4 { "How involved do you want to be tonight?" }
            p {
                "A higher interest level means you'll be more likely to have an important role tonight, and to be more involved."
            }
            input {
                r#type: "number",
                min: "1",
                max: "10",
                value: "{level}",
                oninput: move |e| {
                    if let Ok(n) = e.value().parse::<u8>() {
                        level.set(n.clamp(1, 10));
                    }
                },
            }
            button {
                onclick: move |_| {
                    on_command
                        .call(Command::SubmitInterestLevel {
                            player: my_id,
                            level: level(),
                        });
                },
                if own_interest_level.is_some() {
                    "Update my interest"
                } else {
                    "Submit my interest"
                }
            }
            if let Some(submitted) = own_interest_level {
                p { "You rated your interest: {submitted}. Waiting for setup to finish." }
            }
        }
    }
}

#[component]
fn RosterList(roster: Vec<RosterEntry>) -> Element {
    rsx! {
        h3 { "Roster" }
        ul {
            for entry in roster {
                li { key: "{entry.id.0}", "{entry.name} ({entry.status:?})" }
            }
        }
    }
}

/// A `<select>` listing every roster entry by name, `value` set to the raw
/// player id -- the Host console's recurring "pick a player" control (3
/// call sites: a task's qualifying player, a Servant point award, and the
/// read-only player-page viewer). `on_change` gets the raw `FormEvent`
/// rather than an already-parsed `PlayerId` so each call site keeps full
/// control of what else its change should do.
#[component]
fn PlayerSelect(
    roster: Vec<RosterEntry>,
    placeholder: &'static str,
    on_change: EventHandler<FormEvent>,
) -> Element {
    // Sorted by name -- a review found every roster dropdown listed
    // players in join order, making a specific name slower to find in a
    // 20-30 player list than an alphabetical one would be.
    let mut roster = roster;
    roster.sort_by(|a, b| a.name.cmp(&b.name));
    rsx! {
        select {
            onchange: move |e| on_change.call(e),
            option { value: "", "{placeholder}" }
            for r in roster {
                option { value: "{r.id.0}", "{r.name}" }
            }
        }
    }
}

/// The full identities Cast Out at the Last Denouncement -- rendered
/// identically on `/play` and `/display` (only the heading level differs).
/// See `PlayerView::finale_cast_out_reveal`'s doc comment for why this is
/// public to every viewer, unlike the Host-only `finale_reveal`. Renders
/// nothing while empty (before the Finale's own Denouncement has closed).
#[component]
fn FinaleCastOutReveal(reveal: Vec<PlayerReveal>, heading_level: u8) -> Element {
    if reveal.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            class: "finale-reveal",
            if heading_level == 2 {
                h2 { "The Last Denouncement -- revealed" }
            } else {
                h4 { "The Last Denouncement -- revealed" }
            }
            for p in reveal {
                p {
                    key: "{p.id.0}",
                    strong { "{p.name}" }
                    ": {p.true_faction:?}"
                    if let Some(c) = p.character { ", {character_label(c)}" }
                    if p.converted { " (secretly converted to the Cult)" }
                }
            }
        }
    }
}

/// Every Whistledown post so far, newest first -- shared by `/play`,
/// `/display`, and the Host console (only the heading level differs).
/// Renders nothing while empty.
#[component]
fn WhistledownPosts(posts: Vec<WhistledownPost>, heading_level: u8) -> Element {
    if posts.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            if heading_level == 2 {
                h2 { "Lady Whistledown's Society Papers" }
            } else if heading_level == 3 {
                h3 { "Lady Whistledown's Society Papers" }
            } else {
                h4 { "Lady Whistledown's Society Papers" }
            }
            for post in posts.into_iter().rev() {
                p {
                    key: "{post.round:?}",
                    class: "whistledown-post",
                    "{post.text}"
                }
            }
        }
    }
}

/// The shared round/phase countdown (rules.md's "shared timer") -- read-only
/// rendering shared by `/play`, `/display`, and (alongside its own start/add/
/// clear controls) the Host console. Renders nothing while no timer is
/// running. Once past zero, switches to counting *up* as visible overtime
/// rather than freezing at "0:00" or disappearing -- a stalled countdown
/// with no indication of how far over budget the room actually is would be
/// worse than no timer at all.
#[component]
fn TimerDisplay(remaining_secs: Option<i64>) -> Element {
    let Some(secs) = remaining_secs else {
        return rsx! {};
    };
    let overtime = secs < 0;
    let abs = secs.unsigned_abs();
    let (minutes, seconds) = (abs / 60, abs % 60);
    rsx! {
        p {
            class: if overtime { "warning-text" } else { "" },
            if overtime {
                "Time's up -- {minutes}:{seconds:02} over"
            } else {
                "~{minutes}:{seconds:02} remaining"
            }
        }
    }
}

/// rules.md §1's "Character creation" -- free text, each field capped at
/// 32 characters (enforced here via `maxlength` and, as the real source of
/// truth, by the engine's own `SubmitBio` validation). Doesn't pre-fill
/// from an already-submitted `own_bio` -- resubmitting silently replaces
/// (see `Command::SubmitBio`'s doc comment), so editing just means
/// retyping, matching this Phase 1-era "basic shell" UI's overall level of
/// polish elsewhere.
///
/// Character Name/Real Name/Occupation are required, and Hobbies/Clothing/
/// Skills each need 3-5 filled-in entries (Dalton's own explicit
/// instruction -- see `Bio::first_missing_required_field`/
/// `first_underfilled_category`, the actual source of truth this mirrors
/// client-side purely so a player finds out *before* submitting, not from
/// a rejected round-trip).
#[component]
fn BioForm(
    my_id: PlayerId,
    own_bio: Option<Bio>,
    locked: bool,
    on_command: EventHandler<Command>,
) -> Element {
    let mut character_name = use_signal(String::new);
    let mut real_name = use_signal(String::new);
    let mut occupation = use_signal(String::new);
    let mut hobbies = use_signal(|| std::array::from_fn::<String, 5, _>(|_| String::new()));
    let mut clothing_features =
        use_signal(|| std::array::from_fn::<String, 5, _>(|_| String::new()));
    let mut skills = use_signal(|| std::array::from_fn::<String, 5, _>(|_| String::new()));

    let filled_count =
        |values: &[String; 5]| values.iter().filter(|s| !s.trim().is_empty()).count();
    let is_valid = !character_name().trim().is_empty()
        && !real_name().trim().is_empty()
        && !occupation().trim().is_empty()
        && filled_count(&hobbies()) >= MIN_CATEGORY_ENTRIES
        && filled_count(&clothing_features()) >= MIN_CATEGORY_ENTRIES
        && filled_count(&skills()) >= MIN_CATEGORY_ENTRIES;

    rsx! {
        div {
            h3 { "Your character sheet" }
            if let Some(bio) = &own_bio {
                div {
                    p {
                        "{pascal_case(&bio.character_name)} -- {pascal_case(&bio.occupation)}"
                    }
                    for (label , field) in [
                        ("Hobbies", &bio.hobbies),
                        ("Clothing", &bio.clothing_features),
                        ("Skills", &bio.skills),
                    ]
                    {
                        p {
                            key: "{label}",
                            "{label}: "
                            {field.iter().filter(|s| !s.is_empty()).map(|s| pascal_case(s)).collect::<Vec<_>>().join(", ")}
                        }
                    }
                }
            }
            // Once the game has started, a character sheet is locked --
            // rules.md's task pool and any bio-derived info-checks already
            // read whatever was submitted before setup finalized, so a
            // late edit would silently diverge from what the game itself
            // is already using.
            if locked {
                if own_bio.is_none() {
                    p { "Character sheets are locked now that the game has started." }
                }
            } else {
            input {
                placeholder: "Character name (required)",
                maxlength: "32",
                value: "{character_name}",
                oninput: move |e| character_name.set(e.value()),
            }
            input {
                placeholder: "Real name (required)",
                maxlength: "32",
                value: "{real_name}",
                oninput: move |e| real_name.set(e.value()),
            }
            input {
                placeholder: "Occupation (required)",
                maxlength: "32",
                value: "{occupation}",
                oninput: move |e| occupation.set(e.value()),
            }
            p { "Hobbies (3-5 required):" }
            for i in 0..5 {
                input {
                    key: "hobby-{i}",
                    placeholder: "Hobby {i + 1}",
                    maxlength: "32",
                    value: "{hobbies()[i]}",
                    oninput: move |e| {
                        let mut h = hobbies();
                        h[i] = e.value();
                        hobbies.set(h);
                    },
                }
            }
            p { "Notable clothing features (3-5 required):" }
            for i in 0..5 {
                input {
                    key: "clothing-{i}",
                    placeholder: "Clothing feature {i + 1}",
                    maxlength: "32",
                    value: "{clothing_features()[i]}",
                    oninput: move |e| {
                        let mut c = clothing_features();
                        c[i] = e.value();
                        clothing_features.set(c);
                    },
                }
            }
            p { "Skills (3-5 required):" }
            for i in 0..5 {
                input {
                    key: "skill-{i}",
                    placeholder: "Skill {i + 1}",
                    maxlength: "32",
                    value: "{skills()[i]}",
                    oninput: move |e| {
                        let mut s = skills();
                        s[i] = e.value();
                        skills.set(s);
                    },
                }
            }
            if !is_valid {
                p { class: "error-text",
                    "Fill in Character Name, Real Name, Occupation, and at least {MIN_CATEGORY_ENTRIES} of each of Hobbies, Clothing, and Skills."
                }
            }
            button {
                disabled: !is_valid,
                onclick: move |_| {
                    on_command
                        .call(Command::SubmitBio {
                            player: my_id,
                            bio: Bio {
                                character_name: character_name(),
                                real_name: real_name(),
                                occupation: occupation(),
                                hobbies: hobbies(),
                                clothing_features: clothing_features(),
                                skills: skills(),
                            },
                        });
                },
                if own_bio.is_some() { "Update bio" } else { "Submit bio" }
            }
            }
        }
    }
}

/// Phase 2 scope: one raw-controls panel covering every ability-bearing
/// character, matching the rest of this Phase 1-era "basic shell" UI --
/// see the module doc comment. Bartender's "did it land" is a checkbox the
/// player sets from an actual coin flip at the table rather than the app
/// rolling it itself, the same "keep randomness at the boundary, let a
/// human adjudicate it" choice `CastOut`'s `fallback_replacement` makes.
#[component]
fn AbilityPanel(
    my_id: PlayerId,
    own_character: Option<Character>,
    abilities: AbilityStatus,
    my_info_checks: Vec<InfoCheckDelivery>,
    fellow_cultists: Vec<PlayerId>,
    known_uprising_members: Vec<PlayerId>,
    roster: Vec<RosterEntry>,
    on_command: EventHandler<Command>,
    /// King/Queen only: fires the crown transfer, which the server resolves
    /// to a random eligible Ton player -- see `ClientMsg::TransferKingQueen`'s
    /// doc comment for why this bypasses the generic `on_command` path.
    on_transfer_king_queen: EventHandler<()>,
) -> Element {
    let Some(character) = own_character else {
        return rsx! {};
    };

    let mut target = use_signal(|| None::<u32>);
    // No default -- a review found this silently defaulted to `true`
    // ("it lands"), so a player who didn't consciously flip the coin at
    // the table before submitting would submit an outcome that was never
    // actually decided. Forcing an explicit choice doesn't change what the
    // ability does, just makes it harder to report the wrong coin flip by
    // accident.
    let mut lands = use_signal(|| None::<bool>);
    let mut kind = use_signal(|| InfoQueryKind::IsTheLeader);
    // Cult Leader only, kept separate from `target` above (that one's for
    // the query) -- irreversible and secret, so it gets the same
    // arm-then-confirm pattern the Host's now-removed Convert panel used,
    // resetting the moment the target changes.
    let mut convert_target = use_signal(|| None::<u32>);
    let mut convert_armed = use_signal(|| false);

    let others: Vec<RosterEntry> = roster
        .iter()
        .filter(|r| r.id != my_id && r.status == PlayerStatus::Active)
        .cloned()
        .collect();
    let target_picker = rsx! {
        select {
            onchange: move |e| target.set(e.value().parse().ok()),
            option { value: "", "-- choose --" }
            for r in others.clone() {
                option { value: "{r.id.0}", "{r.name}" }
            }
        }
    };
    let convert_target_picker = rsx! {
        select {
            onchange: move |e| {
                convert_target.set(e.value().parse().ok());
                convert_armed.set(false);
            },
            option { value: "", "-- choose --" }
            for r in others.clone() {
                option { value: "{r.id.0}", "{r.name}" }
            }
        }
    };

    rsx! {
        div {
            h3 { "Your ability" }
            p { "{ability_description(character)}" }
            if !fellow_cultists.is_empty() {
                p { "Fellow Cultists: {names(&fellow_cultists, &roster)}" }
            }
            if !known_uprising_members.is_empty() {
                p { "Uprising members you know: {names(&known_uprising_members, &roster)}" }
            }
            if !my_info_checks.is_empty() {
                h4 { "Your info-check results" }
                ul {
                    for (i , check) in my_info_checks.iter().enumerate() {
                        li { key: "{i}", "{describe_check(check, &roster)}" }
                    }
                }
            }
            match character {
                Character::Oracle => rsx! {
                    p { "Checks available: {abilities.oracle_checks_available.unwrap_or(0)}" }
                    {target_picker}
                    button {
                        disabled: abilities.oracle_checks_available.unwrap_or(0) == 0 || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::UseOracle { player: my_id, target: PlayerId(t) });
                        },
                        "View full history",
                    }
                },
                Character::Almanac => rsx! {
                    button {
                        disabled: !abilities.almanac_available.unwrap_or(false),
                        onclick: move |_| on_command.call(Command::UseAlmanac { player: my_id }),
                        "Learn 3 non-Leaders",
                    }
                },
                Character::Spymaster => rsx! {
                    {target_picker}
                    button {
                        disabled: !abilities.spymaster_available.unwrap_or(false) || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::UseSpymaster { player: my_id, target: PlayerId(t) });
                        },
                        "View faction color",
                    }
                },
                Character::CultLeader => rsx! {
                    p { "Queries available: {abilities.cult_leader_queries_available.unwrap_or(0)}" }
                    p {
                        if abilities.recruitment_slots_available.unwrap_or(0) == 0 {
                            "No recruitment slot available right now -- wait for the next one to open."
                        } else {
                            "Recruitment slots available: {abilities.recruitment_slots_available.unwrap_or(0)}."
                        }
                    }
                    {target_picker}
                    select {
                        onchange: move |e| kind.set(if e.value() == "IsTonAligned" {
                            InfoQueryKind::IsTonAligned
                        } else {
                            InfoQueryKind::IsTheLeader
                        }),
                        option { value: "IsTheLeader", "Is this the Leader?" }
                        option { value: "IsTonAligned", "Is this Ton-aligned?" }
                    }
                    button {
                        disabled: abilities.cult_leader_queries_available.unwrap_or(0) == 0 || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command
                                .call(Command::CultLeaderQuery {
                                    player: my_id,
                                    target: PlayerId(t),
                                    kind: kind(),
                                });
                        },
                        "Query",
                    }
                    h4 { "Convert" }
                    p { "Irreversible and secret -- double check before confirming." }
                    {convert_target_picker}
                    button {
                        disabled: abilities.recruitment_slots_available.unwrap_or(0) == 0 || convert_target().is_none(),
                        onclick: move |_| {
                            let Some(t) = convert_target() else { return };
                            if !convert_armed() {
                                convert_armed.set(true);
                                return;
                            }
                            on_command
                                .call(Command::Convert { converter: my_id, target: PlayerId(t) });
                            convert_armed.set(false);
                            convert_target.set(None);
                        },
                        if convert_armed() { "Confirm convert -- click again" } else { "Convert" }
                    }
                },
                Character::Deceiver => rsx! {
                    p {
                        if abilities.deceiver_falsify_used.unwrap_or(false) {
                            "Already used your falsify."
                        } else if abilities.deceiver_armed.unwrap_or(false) {
                            "Armed -- the next check against you will be falsified."
                        } else {
                            "Not armed."
                        }
                    }
                    button {
                        disabled: abilities.deceiver_falsify_used.unwrap_or(false),
                        onclick: move |_| {
                            let armed = !abilities.deceiver_armed.unwrap_or(false);
                            on_command.call(Command::SetDeceiverArmed { player: my_id, armed });
                        },
                        if abilities.deceiver_armed.unwrap_or(false) { "Disarm" } else { "Arm" }
                    }
                },
                Character::PriestPriestess => rsx! {
                    p { "Protects available: {abilities.priest_protects_available.unwrap_or(0)}" }
                    {target_picker}
                    button {
                        disabled: abilities.priest_protects_available.unwrap_or(0) == 0 || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::PriestProtect { player: my_id, target: PlayerId(t) });
                        },
                        "Protect from conversion",
                    }
                },
                Character::DoctorMedic => rsx! {
                    p {
                        if abilities.medic_available.unwrap_or(false) {
                            "A Denouncement is open -- you can protect someone."
                        } else {
                            "No Denouncement is currently open."
                        }
                    }
                    {target_picker}
                    button {
                        disabled: !abilities.medic_available.unwrap_or(false) || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::MedicProtect { player: my_id, target: PlayerId(t) });
                        },
                        "Protect from Cast-Out",
                    }
                },
                Character::Bartender => rsx! {
                    p {
                        if abilities.bartender_available.unwrap_or(false) {
                            "Available this round."
                        } else {
                            "Already used this round."
                        }
                    }
                    {target_picker}
                    p { "Flip a coin at the table, then record what actually happened:" }
                    label {
                        input {
                            r#type: "radio",
                            name: "bartender-lands",
                            checked: lands() == Some(true),
                            onchange: move |_| lands.set(Some(true)),
                        }
                        " It landed"
                    }
                    label {
                        input {
                            r#type: "radio",
                            name: "bartender-lands",
                            checked: lands() == Some(false),
                            onchange: move |_| lands.set(Some(false)),
                        }
                        " It didn't land"
                    }
                    button {
                        disabled: !abilities.bartender_available.unwrap_or(false)
                            || target().is_none()
                            || lands().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            let Some(l) = lands() else { return };
                            on_command
                                .call(Command::BartenderTarget {
                                    player: my_id,
                                    target: PlayerId(t),
                                    lands: l,
                                });
                            lands.set(None);
                        },
                        "Target",
                    }
                },
                Character::PotionMaker => rsx! {
                    {target_picker}
                    button {
                        disabled: !abilities.potion_maker_available.unwrap_or(false) || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::ActivatePotionImmunity { player: my_id, target: PlayerId(t) });
                        },
                        "Protect from execution",
                    }
                },
                Character::Magistrate | Character::Firebrand => rsx! {
                    button {
                        disabled: !abilities.double_vote_available.unwrap_or(false),
                        onclick: move |_| on_command.call(Command::ActivateDoubleVote { player: my_id }),
                        "Arm double vote for this ballot",
                    }
                },
                Character::NormalUprising => rsx! {
                    button {
                        disabled: !abilities.vote_shield_available.unwrap_or(false),
                        onclick: move |_| on_command.call(Command::ArmVoteShield { player: my_id }),
                        "Shield yourself from one vote",
                    }
                },
                Character::Duelist => rsx! {
                    p { "Guarantees a ballot spot -- use only while nomination is open." }
                    {target_picker}
                    button {
                        disabled: !abilities.duelist_available.unwrap_or(false) || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::DuelistChallenge { player: my_id, target: PlayerId(t) });
                        },
                        "Challenge",
                    }
                },
                Character::Agitator => rsx! {
                    p { "Adds a target to the ballot -- use only while discussion is open." }
                    {target_picker}
                    button {
                        disabled: !abilities.agitator_available.unwrap_or(false) || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::AgitatorRedirect { player: my_id, target: PlayerId(t) });
                        },
                        "Redirect",
                    }
                },
                Character::GrandInquisitor => rsx! {
                    p { "Forces exactly 2 Cast-Outs this Denouncement, regardless of headcount." }
                    button {
                        disabled: !abilities.grand_inquisitor_available.unwrap_or(false),
                        onclick: move |_| on_command.call(Command::ActivateGrandInquisitor { player: my_id }),
                        "Invoke the office",
                    }
                },
                Character::KingQueen => rsx! {
                    p {
                        if abilities.king_queen_transfer_available.unwrap_or(false) {
                            "You may transfer the crown once, before Round 5 -- to a random eligible Ton player, unmasking both of you."
                        } else {
                            "Transfer already used, or it's Round 5 or later."
                        }
                    }
                    button {
                        disabled: !abilities.king_queen_transfer_available.unwrap_or(false),
                        onclick: move |_| on_transfer_king_queen.call(()),
                        "Transfer the crown",
                    }
                },
                Character::RevolutionaryLeader => rsx! {
                    p {
                        if let Some(successor) = abilities.designated_successor {
                            "Currently designated: {names(&[successor], &roster)}"
                        } else {
                            "No successor designated yet -- if you're Cast Out with nobody chosen, it defaults to a random remaining Uprising member."
                        }
                    }
                    {target_picker}
                    button {
                        disabled: target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command.call(Command::DesignateSuccessor { leader: my_id, successor: PlayerId(t) });
                        },
                        "Designate successor",
                    }
                },
                _ => rsx! {},
            }
        }
    }
}

fn describe_check(check: &InfoCheckDelivery, roster: &[RosterEntry]) -> String {
    let target = check
        .target
        .map(|t| names(&[t], roster))
        .unwrap_or_else(|| "no single target".to_string());
    match &check.answer {
        InfoCheckAnswer::Dossier(d) => {
            let character = d.character.map(character_label).unwrap_or("no title yet");
            let converted = if d.converted { "yes" } else { "no" };
            format!(
                "{target} -- apparent faction: {:?}, converted: {converted}, character: {character}",
                d.apparent_faction
            )
        }
        InfoCheckAnswer::Faction(f) => format!("{target}: faction color {f:?}"),
        // `kind` distinguishes the Cult Leader's two possible yes/no
        // queries (IsTheLeader vs. IsTonAligned) -- without it, two
        // queries against different players/questions would render as
        // indistinguishable "Name: true/false" lines.
        InfoCheckAnswer::Bool(b) => {
            let question = match check.kind {
                InfoQueryKind::IsTheLeader => "is the Revolutionary Leader",
                InfoQueryKind::IsTonAligned => "is Ton-aligned",
                _ => "?",
            };
            let answer = if *b { "yes" } else { "no" };
            format!("{target} {question}: {answer}")
        }
        InfoCheckAnswer::PlayerSet(set) => {
            format!("Definitely not the Leader: {}", names(set, roster))
        }
    }
}

#[component]
fn DenouncementPanel(
    my_id: PlayerId,
    denouncement: Option<DenouncementView>,
    roster: Vec<RosterEntry>,
    on_command: EventHandler<Command>,
) -> Element {
    let Some(phase) = denouncement else {
        return rsx! { p { "No Denouncement in progress." } };
    };
    // A review found the Ballot and Runoff phases rendered identically
    // (same "Ballot" heading, same copy) -- a player who voted in a first
    // ballot that tied, then sees the exact same names reappear, had no
    // way to tell this is a runoff (narrower field, real stakes) rather
    // than a glitchy repeat. Checked against a reference so it doesn't
    // consume `phase` before the match below does.
    let is_runoff = matches!(&phase, DenouncementView::Runoff { .. });

    let mut active_others: Vec<RosterEntry> = roster
        .into_iter()
        .filter(|r| r.status == PlayerStatus::Active && r.id != my_id)
        .collect();
    active_others.sort_by(|a, b| a.name.cmp(&b.name));

    match phase {
        DenouncementView::Nomination { i_have_acted } => {
            let mut pick = use_signal(|| None::<u32>);
            let options = active_others.clone();
            rsx! {
                h3 { "Nomination" }
                p { if i_have_acted { "You've nominated someone." } else { "Choose who to nominate." } }
                select {
                    onchange: move |e| pick.set(e.value().parse().ok()),
                    option { value: "", "-- choose --" }
                    for r in options {
                        option { value: "{r.id.0}", "{r.name}" }
                    }
                }
                button {
                    disabled: pick().is_none(),
                    onclick: move |_| {
                        let Some(raw) = pick() else { return };
                        let nominee = PlayerId(raw);
                        on_command.call(Command::Nominate { voter: my_id, nominee });
                    },
                    "Nominate"
                }
            }
        }
        DenouncementView::Discussion { surfaced } => rsx! {
            h3 { "Discussion" }
            p { "Up for the Denouncement: {names(&surfaced, &active_others)}" }
        },
        DenouncementView::Ballot {
            candidates,
            i_have_acted,
        }
        | DenouncementView::Runoff {
            candidates,
            i_have_acted,
        } => {
            let mut pick = use_signal(|| None::<u32>);
            let options: Vec<RosterEntry> = active_others
                .iter()
                .filter(|r| candidates.contains(&r.id))
                .cloned()
                .collect();
            rsx! {
                if is_runoff {
                    h3 { "Runoff -- the first ballot tied" }
                    p {
                        if i_have_acted { "You've voted in the runoff." } else { "Vote again to break the tie." }
                    }
                } else {
                    h3 { "Ballot" }
                    p { if i_have_acted { "You've voted." } else { "Cast your ballot." } }
                }
                select {
                    onchange: move |e| pick.set(e.value().parse().ok()),
                    option { value: "", "-- choose --" }
                    for r in options {
                        option { value: "{r.id.0}", "{r.name}" }
                    }
                }
                button {
                    disabled: pick().is_none(),
                    onclick: move |_| {
                        let Some(raw) = pick() else { return };
                        let ballot = Ballot::For(PlayerId(raw));
                        on_command.call(Command::CastBallot { voter: my_id, ballot });
                    },
                    "Vote"
                }
                button {
                    onclick: move |_| {
                        on_command.call(Command::CastBallot { voter: my_id, ballot: Ballot::Abstain });
                    },
                    "Abstain"
                }
            }
        }
    }
}

fn names(ids: &[PlayerId], roster: &[RosterEntry]) -> String {
    ids.iter()
        .map(|id| {
            roster
                .iter()
                .find(|r| r.id == *id)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| format!("#{}", id.0))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A human-readable label for every `Character` variant -- a review pass
/// found the player's own reveal rendered these via `{c:?}` (e.g. a bare
/// "PrincePrincess", no space), inconsistent with the Host's own
/// character-assign dropdown, which already spells the 4 titled roles out
/// properly. This extends that same treatment to the rest of the roster so
/// every player's reveal reads as prose, not a Rust identifier.
fn character_label(c: Character) -> &'static str {
    match c {
        Character::KingQueen => "King/Queen",
        Character::PrincePrincess => "Prince/Princess",
        Character::RevolutionaryLeader => "Revolutionary Leader",
        Character::CultLeader => "Cult Leader",
        Character::Oracle => "Oracle",
        Character::Almanac => "Almanac",
        Character::Spymaster => "Spymaster",
        Character::PriestPriestess => "Priest/Priestess",
        Character::PotionMaker => "Potion Maker",
        Character::Magistrate => "Magistrate",
        Character::Bartender => "Bartender",
        Character::DoctorMedic => "Doctor/Medic",
        Character::Firebrand => "Firebrand",
        Character::CellLeader => "Cell Leader",
        Character::Deceiver => "Deceiver",
        Character::Duelist => "Duelist",
        Character::Agitator => "Agitator",
        Character::GrandInquisitor => "Grand Inquisitor",
        Character::NormalTon => "Ton",
        Character::NormalUprising => "Uprising",
        Character::Cultist => "Cultist",
    }
}

/// One line of narrative flavor per faction -- rules.md §0's own framing
/// of the three-way conflict, plus the Servants' separate track (§1).
/// Shown alongside a player's reveal so "Ton" or "Cult" reads as a real
/// stake in the night, not just a color-coded label.
fn faction_flavor(faction: Faction) -> &'static str {
    match faction {
        Faction::Ton => "High society -- trying to root out the agitator undermining it.",
        Faction::Uprising => {
            "A movement trying to survive the night and keep its leadership intact."
        }
        Faction::Cult => {
            "A hidden third faction, secretly steering both of the above toward its own ends."
        }
        Faction::Servant => {
            "A separate, non-competing track tonight, outside the three-way conflict."
        }
        Faction::Unassigned => "Not yet assigned.",
    }
}

/// One line of narrative flavor per character -- each role's own "Goal"
/// column and narrative title from rules.md §3.1-3.3, in second person.
/// Distinct from `ability_description` below: this is *why* the role
/// exists in the story, not *what button it presses*.
fn character_flavor(character: Character) -> &'static str {
    match character {
        Character::KingQueen => "Your goal: avoid conversion to the Cult.",
        Character::PrincePrincess => {
            "Known as \"the Heir.\" Your goal: protect the King/Queen."
        }
        Character::RevolutionaryLeader => {
            "Your goal: survive to the end. Not even your own faction knows who you are at the start."
        }
        Character::CultLeader => "Your goal: achieve any of the Cult's four win paths.",
        Character::Oracle => "Your goal: find the Revolutionary Leader.",
        Character::Almanac => "Your goal: narrow the field by elimination.",
        Character::Spymaster => "Your goal: identify threats.",
        Character::PriestPriestess => {
            "Known as \"the Chaperone/Confessor.\" Your goal: protect the King/Queen."
        }
        Character::PotionMaker => "Known as \"the Modiste.\" Your goal: protect a target from the vote.",
        Character::Magistrate => "Your goal: ensure the Denouncement lands correctly.",
        Character::Bartender => "A footman/valet. Your goal: disrupt threats to the Leader.",
        Character::DoctorMedic => "Your goal: protect the Leader.",
        Character::Firebrand => "Your goal: rally the Uprising's numbers.",
        Character::CellLeader => {
            "Your goal: coordinate the rank-and-file without exposing the true Leader."
        }
        Character::Deceiver => "Your goal: protect the Cult's cover under scrutiny.",
        Character::Duelist => "Your goal: force a suspect to face judgment.",
        Character::Agitator => "Your goal: protect the movement through misdirection.",
        Character::GrandInquisitor => {
            "Your goal: press the Ton's advantage at a critical Denouncement."
        }
        Character::NormalTon => "A member of high society -- no named role, but never underestimate a crowd.",
        Character::NormalUprising => "A member of the movement -- no named role, but every voice counts.",
        Character::Cultist => "A secretly recruited member of the Cult. Your goal: support the Cult Leader.",
    }
}

/// The mechanical "what does my ability actually do" text for `AbilityPanel`'s
/// "Your ability" section -- adapted from rules.md §3.1-3.3's own "Ability"
/// column into second person, one entry per character regardless of
/// whether that character also gets interactive controls below it (a
/// purely passive ability, like the Cell Leader's or Cultist's, still gets
/// an explanation here, just no button). A review found this section
/// existed as a heading with nothing under it -- every character's
/// control (where one exists) used to be the only explanation of what it
/// did.
fn ability_description(character: Character) -> &'static str {
    match character {
        Character::KingQueen => "Once per game, before Round 5, you may transfer the crown to a random remaining Ton player -- unmasking you both.",
        Character::PrincePrincess => "You automatically learn the King/Queen's identity once Round 3 begins -- no action needed.",
        Character::RevolutionaryLeader => "At any time, you may secretly pre-designate a successor. If you're Cast Out with nobody chosen, succession defaults to a random remaining Uprising member.",
        Character::CultLeader => "Before each recruitment window, you may query one candidate -- are they Ton-aligned, or are they the Revolutionary Leader? You also designate which recruited Cultist holds the Deceiver title.",
        Character::Cultist => "You know your fellow Cultists. No active ability beyond that.",
        Character::Oracle => "After every odd round, you may view one player's full history, locked at that moment. Permanently disabled if the King/Queen is Cast Out.",
        Character::Almanac => "Once per game, you privately learn 3 players who are definitely not the Revolutionary Leader.",
        Character::Spymaster => "Once per game, you view a single player's faction color only.",
        Character::PriestPriestess => "Once per Cult recruitment window, you may protect one person from conversion, without knowing that's what you're protecting against. You can't protect the same person twice all game.",
        Character::PotionMaker => "Once per game, you may grant execution-immunity, saving whoever the public vote would Cast Out that round.",
        Character::Magistrate => "Once per game, your ballot counts as two votes at tally.",
        Character::Firebrand => "Once per game, your ballot counts as two votes at tally -- the Uprising's mirror to the Magistrate.",
        Character::Bartender => "Once per round, you may target someone with a 50% chance of making them drunk that round. You're never told whether it actually landed.",
        Character::DoctorMedic => "Once per round, you may protect one person; if they're selected for Cast-Out, they're removed from the resolved list before slots are filled. You can't protect the same person on two consecutive rounds.",
        Character::Duelist => "Once per game, before nomination closes, you may \"challenge\" one player -- guaranteeing them a spot on the ballot regardless of verbal support.",
        Character::Agitator => "Once per game, during discussion, you may force the room to spend extra time debating a different player of your choosing instead.",
        Character::GrandInquisitor => "Once per game, before a ballot closes, you may invoke your office: both of the top two vote-getters are Cast Out that round, regardless of the standard execution-count rule.",
        Character::NormalTon => "You auto-succeed one failed social task, once per game -- automatic, no action needed.",
        Character::NormalUprising => "Once per game, you may shield yourself, ignoring one vote cast against you.",
        Character::CellLeader => "You know 2 other Uprising members (never the Leader). No active ability beyond that.",
        Character::Deceiver => "Once per game, if targeted by another player's info-check ability, you may force that check to return a false result.",
    }
}

/// Every populated `AbilityStatus` field as a plain, human-readable line --
/// used only by `PlayerPageReadOnly`. Generic over every character rather
/// than a per-character match like `AbilityPanel`'s (deliberately, since
/// this is read-only display text, not action controls -- there's no
/// target picker or button to build per character here, just "what does
/// the status struct currently say").
fn ability_status_lines(status: &AbilityStatus, roster: &[RosterEntry]) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(n) = status.oracle_checks_available {
        lines.push(format!("Oracle checks available: {n}"));
    }
    if let Some(b) = status.almanac_available {
        lines.push(format!("Almanac available: {b}"));
    }
    if let Some(b) = status.spymaster_available {
        lines.push(format!("Spymaster available: {b}"));
    }
    if let Some(n) = status.cult_leader_queries_available {
        lines.push(format!("Cult Leader queries available: {n}"));
    }
    if let Some(n) = status.recruitment_slots_available {
        lines.push(format!("Recruitment slots available: {n}"));
    }
    if let Some(b) = status.deceiver_armed {
        lines.push(format!("Deceiver armed: {b}"));
    }
    if let Some(b) = status.deceiver_falsify_used {
        lines.push(format!("Deceiver falsify used: {b}"));
    }
    if let Some(n) = status.priest_protects_available {
        lines.push(format!("Priest/Priestess protects available: {n}"));
    }
    if let Some(b) = status.medic_available {
        lines.push(format!("Medic protect available: {b}"));
    }
    if let Some(b) = status.bartender_available {
        lines.push(format!("Bartender available: {b}"));
    }
    if let Some(b) = status.potion_maker_available {
        lines.push(format!("Potion Maker available: {b}"));
    }
    if let Some(b) = status.double_vote_available {
        lines.push(format!("Double vote available: {b}"));
    }
    if let Some(b) = status.vote_shield_available {
        lines.push(format!("Vote shield available: {b}"));
    }
    if let Some(b) = status.duelist_available {
        lines.push(format!("Duelist challenge available: {b}"));
    }
    if let Some(b) = status.agitator_available {
        lines.push(format!("Agitator redirect available: {b}"));
    }
    if let Some(b) = status.grand_inquisitor_available {
        lines.push(format!("Grand Inquisitor invoke available: {b}"));
    }
    if let Some(b) = status.king_queen_transfer_available {
        lines.push(format!("King/Queen transfer available: {b}"));
    }
    if let Some(successor) = status.designated_successor {
        lines.push(format!(
            "Designated successor: {}",
            names(&[successor], roster)
        ));
    }
    lines
}

/// Host-only, read-only mirror of what a specific player currently sees on
/// `/play` -- see `ClientMsg::ViewPlayer`'s doc comment. Renders from the
/// exact same `PlayerView` that player's own connection would get (there's
/// no separate "admin projection"), so this can never show Dalton more
/// than that player already sees themselves.
///
/// Deliberately renders no buttons, forms, `<select>`s, or `on_command`
/// handlers of any kind -- text and lists only. This is what actually
/// enforces "no edits from this view": it isn't that controls are present
/// but disabled, it's that there are no controls here that could ever
/// issue a `Command` at all. Compare `AbilityPanel`/`DenouncementPanel`/
/// `TaskAttemptForm`, which this deliberately does NOT reuse, since all
/// three exist specifically to submit commands.
#[component]
fn PlayerPageReadOnly(id: PlayerId, v: PlayerView) -> Element {
    let is_servant = v.own_faction == Some(Faction::Servant);
    let is_cast_out = v
        .roster
        .iter()
        .any(|r| r.id == id && r.status == PlayerStatus::CastOut);

    rsx! {
        div {
            class: "readonly-player-view",
            p { style: "font-style:italic;", "Viewing {names(&[id], &v.roster)}'s page -- round: {v.current_round:?}" }
            if !v.raffle_closed {
                p { "Still in Character Creation -- hasn't been assigned a role yet." }
                if let Some(level) = v.own_interest_level {
                    p { "Interest level: {level}" }
                }
            }
            if let Some(faction) = v.own_faction {
                p {
                    "Faction: {faction:?}"
                    if let Some(c) = v.own_character {
                        " -- {character_label(c)}"
                    }
                }
            }

            h4 { "Character" }
            if is_servant {
                p { "{faction_flavor(Faction::Servant)}" }
                p { "Joined after the game started, so there's no character sheet or ability panel." }
            } else {
                if let Some(faction) = v.own_faction {
                    if faction != Faction::Unassigned {
                        p { "{faction_flavor(faction)}" }
                    }
                }
                if let Some(c) = v.own_character {
                    p { "{character_flavor(c)}" }
                    h4 { "Ability" }
                    p { "{ability_description(c)}" }
                    for line in ability_status_lines(&v.my_abilities, &v.roster) {
                        p { "{line}" }
                    }
                }
                if !v.fellow_cultists.is_empty() {
                    p { "Fellow Cultists: {names(&v.fellow_cultists, &v.roster)}" }
                }
                if !v.known_uprising_members.is_empty() {
                    p { "Uprising members known: {names(&v.known_uprising_members, &v.roster)}" }
                }
                if !v.my_info_checks.is_empty() {
                    h4 { "Info-check results" }
                    ul {
                        for (i , check) in v.my_info_checks.iter().enumerate() {
                            li { key: "{i}", "{describe_check(check, &v.roster)}" }
                        }
                    }
                }
                if let Some(king_queen) = v.known_king_queen {
                    p { "Knows the King/Queen: {names(&[king_queen], &v.roster)}" }
                }
                if let Some(leader) = v.revealed_leader {
                    p { "Knows the Revolutionary Leader: {names(&[leader], &v.roster)}" }
                }
                if !v.my_confidants.is_empty() {
                    p { "Confidants who know they're the Leader: {names(&v.my_confidants, &v.roster)}" }
                }
                if let Some(message) = &v.martyrdom_message {
                    p { class: "martyrdom-message", "{message}" }
                }
            }
            BioForm { my_id: id, own_bio: v.own_bio.clone(), locked: true, on_command: |_| {} }

            h4 { "Round" }
            if v.i_am_drunk {
                p { class: "error-text", "Drunk this round -- can't nominate or vote." }
            }
            if v.open_tasks.is_empty() {
                p { "No open tasks." }
            } else {
                ul {
                    for task in v.open_tasks.iter() {
                        li {
                            key: "{task.id.0}",
                            "{task.prompt} ({task.tier:?})"
                            if task.is_location_task { " -- location task" }
                            match task.my_outcome {
                                Some(true) => " -- completed",
                                Some(false) => " -- no match this time",
                                None => " -- not yet attempted",
                            }
                        }
                    }
                }
            }
            if is_cast_out {
                p { "Cast Out -- spectating this Denouncement." }
            }
            match &v.denouncement {
                None => rsx! { p { "No Denouncement currently open." } },
                Some(DenouncementView::Nomination { .. }) => rsx! { p { "Nomination is open." } },
                Some(DenouncementView::Discussion { surfaced }) => rsx! {
                    p { "Discussion -- surfaced: {names(surfaced, &v.roster)}" }
                },
                Some(DenouncementView::Ballot { candidates, .. }) => rsx! {
                    p { "Ballot open -- candidates: {names(candidates, &v.roster)}" }
                },
                Some(DenouncementView::Runoff { candidates, .. }) => rsx! {
                    p { "Runoff open -- candidates: {names(candidates, &v.roster)}" }
                },
            }

            h4 { "Game" }
            if let Some(entrants) = &v.intermission_entrants {
                p { "Intermission entrants: {names(entrants, &v.roster)}" }
            } else if v.i_opted_into_intermission {
                p { "Opted into the Intermission lottery." }
            }
            if !v.servant_leaderboard.is_empty() {
                h4 { "Servant leaderboard" }
                ul {
                    for (pid , points) in v.servant_leaderboard.iter() {
                        li { key: "{pid.0}", "{names(&[*pid], &v.roster)}: {points}" }
                    }
                }
            }
            WhistledownPosts { posts: v.whistledown.clone(), heading_level: 5u8 }
        }
    }
}

/// Every named `Character`, grouped by the faction the engine's own
/// (private) `required_faction` requires for it -- mirrors that match
/// exactly, duplicated here since `engine` has no public "every character
/// in this faction" accessor to reuse instead. `NormalTon`/
/// `NormalUprising`/`Cultist` close out each of their factions' lists --
/// unlike the rest, many players can hold these at once.
const TON_CHARACTERS: &[Character] = &[
    Character::KingQueen,
    Character::PrincePrincess,
    Character::Oracle,
    Character::Almanac,
    Character::PriestPriestess,
    Character::PotionMaker,
    Character::Magistrate,
    Character::Duelist,
    Character::GrandInquisitor,
    Character::NormalTon,
];
const UPRISING_CHARACTERS: &[Character] = &[
    Character::RevolutionaryLeader,
    Character::Spymaster,
    Character::Bartender,
    Character::DoctorMedic,
    Character::Firebrand,
    Character::CellLeader,
    Character::Agitator,
    Character::NormalUprising,
];
const CULT_CHARACTERS: &[Character] = &[
    Character::CultLeader,
    Character::Deceiver,
    Character::Cultist,
];

/// `character_label` reads oddly for the 3 generic catch-all roles on this
/// page specifically ("Ton" right under a "Ton" faction heading) -- everywhere
/// else that label appears next to a specific player's own reveal, where the
/// faction is already obvious from context and doesn't need repeating.
fn role_reference_label(c: Character) -> &'static str {
    match c {
        Character::NormalTon => "Normal Ton member",
        Character::NormalUprising => "Normal Uprising member",
        Character::Cultist => "Cultist (unnamed recruit)",
        other => character_label(other),
    }
}

/// Host-only role compendium: every character in the game, assigned or not,
/// with its flavor text and ability description -- a setup-time reference
/// (what does each role actually do, have I already handed this one out)
/// and a narration aid during the night. Shows only whether a role has
/// been given to *someone* (`assigned`, from
/// `GameState::assigned_characters`), never who -- rules.md's own framing
/// never gives the Host an ambient view of who's who, and this page isn't
/// the deliberate, scoped exception the Finale reveal is.
#[component]
fn RoleReference(assigned: Vec<Character>) -> Element {
    let groups: [(Faction, &[Character]); 3] = [
        (Faction::Ton, TON_CHARACTERS),
        (Faction::Uprising, UPRISING_CHARACTERS),
        (Faction::Cult, CULT_CHARACTERS),
    ];
    rsx! {
        div { class: "role-reference",
            for (faction , characters) in groups {
                div {
                    key: "{faction:?}",
                    h4 { "{faction:?}" }
                    p { style: "font-style:italic;", "{faction_flavor(faction)}" }
                    for c in characters.iter().copied() {
                        div {
                            key: "{role_reference_label(c)}",
                            class: "role-reference-entry",
                            p {
                                strong { "{role_reference_label(c)}" }
                                " -- "
                                if assigned.contains(&c) { "Assigned" } else { "Not yet assigned" }
                            }
                            p { "{character_flavor(c)}" }
                            p { "{ability_description(c)}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TaskAttemptForm(
    my_id: PlayerId,
    task: TaskView,
    roster: Vec<RosterEntry>,
    on_command: EventHandler<Command>,
) -> Element {
    let mut options: Vec<RosterEntry> = roster
        .into_iter()
        .filter(|r| r.status == PlayerStatus::Active && r.id != my_id)
        .collect();
    options.sort_by(|a, b| a.name.cmp(&b.name));

    if let Some(credited) = task.my_outcome {
        return rsx! {
            div {
                h4 { "{task.prompt} ({task.tier:?})" }
                p { if credited { "Completed!" } else { "No match this time." } }
            }
        };
    }

    let first = use_signal(|| None::<u32>);
    let second = use_signal(|| None::<u32>);
    let third = use_signal(|| None::<u32>);

    let picks = (first(), second(), third());
    let all_distinct = match picks {
        (Some(a), Some(b), Some(c)) => a != b && b != c && a != c,
        _ => false,
    };
    let task_id = task.id;

    rsx! {
        div {
            h4 { "{task.prompt} ({task.tier:?})" }
            p { "Name 3 people you talked to:" }
            for (slot, mut setter) in [(first(), first), (second(), second), (third(), third)] {
                select {
                    onchange: move |e| setter.set(e.value().parse().ok()),
                    option { value: "", "-- choose --" }
                    for r in options.clone() {
                        option { selected: slot == Some(r.id.0), value: "{r.id.0}", "{r.name}" }
                    }
                }
            }
            if picks.0.is_some() && picks.1.is_some() && picks.2.is_some() && !all_distinct {
                p { "Pick three different people." }
            }
            button {
                disabled: !all_distinct,
                onclick: move |_| {
                    let (Some(a), Some(b), Some(c)) = (first(), second(), third()) else { return };
                    let named = [PlayerId(a), PlayerId(b), PlayerId(c)];
                    on_command.call(Command::AttemptTask { player: my_id, task: task_id, named });
                },
                "Submit"
            }
        }
    }
}

/// rules.md §4's location tasks (medium: the location stated plainly;
/// hard: a riddle as to where it is) -- credited once the player enters
/// the code physically placed there (`Command::AttemptLocationTask`,
/// checked trimmed/case-insensitive). Doesn't take a `roster` the way
/// `TaskAttemptForm` does -- there's no one to name, just a code to type.
#[component]
fn LocationTaskAttemptForm(
    my_id: PlayerId,
    task: TaskView,
    on_command: EventHandler<Command>,
) -> Element {
    if let Some(credited) = task.my_outcome {
        return rsx! {
            div {
                h4 { "{task.prompt} ({task.tier:?})" }
                p { if credited { "Correct code -- completed!" } else { "Wrong code -- that attempt is used up." } }
            }
        };
    }

    let mut code = use_signal(String::new);
    let task_id = task.id;

    rsx! {
        div {
            h4 { "{task.prompt} ({task.tier:?})" }
            p { "Find the code at the location and enter it below. You only get one attempt, so double-check it before submitting." }
            input {
                placeholder: "Code",
                value: "{code}",
                oninput: move |e| code.set(e.value()),
            }
            button {
                disabled: code().trim().is_empty(),
                onclick: move |_| {
                    let code = code.peek().trim().to_string();
                    on_command.call(Command::AttemptLocationTask { player: my_id, task: task_id, code });
                },
                "Submit code"
            }
        }
    }
}

// --- /host -----------------------------------------------------------------

#[component]
fn Host() -> Element {
    let mut view = use_signal(|| None::<PlayerView>);
    let mut error = use_signal(|| None::<String>);
    let mut location_task_templates = use_signal(Vec::<(usize, TaskTier, String)>::new);
    let mut timer = use_signal(|| None::<i64>);
    // Gates the whole console behind `game_server::check_host_password` --
    // the server itself now enforces this too (`command_authorized`/the
    // `Watch`/`ViewPlayer`/etc. checks in `game_ws`), not just this UI
    // choosing not to render controls before login succeeds, but this
    // local gate still exists on its own merits: it keeps Host-privileged
    // data from even being *requested* without the passphrase, not merely
    // rejected once asked for. Deliberately doesn't persist across a
    // reload (no localStorage) -- simpler and lower-risk than adding a new
    // browser API dependency this session can't test end to end;
    // re-entering the passphrase once per reload is a small, known,
    // accepted tradeoff.
    let mut authed = use_signal(|| false);
    let mut login_failed = use_signal(|| false);
    let mut password_draft = use_signal(String::new);
    // The "view a player's page" panel -- see `ClientMsg::ViewPlayer`'s
    // doc comment. `None` means the panel is closed/not requested.
    let mut viewed_player: Signal<Option<PlayerView>> = use_signal(|| None);
    // A reliability review found a dropped connection here gives zero
    // visible sign -- the receive loop just quietly stops and every
    // subsequent button click silently does nothing, which is worst for
    // exactly this route: the Host is the one person running the whole
    // live event, and every phase-advancing action funnels through this
    // one tab. This is a visibility fix only, not real reconnect support
    // (see the module doc comment's SECOND KNOWN GAP) -- the fix is
    // telling Dalton to reload, not attempting to recover automatically.
    let mut connected = use_signal(|| true);
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        loop {
            match socket.recv().await {
                // Deliberately NOT clearing `error` here -- see `Play`'s
                // identical comment on the same fix. Every Host-visible
                // action below clears it locally instead.
                Ok(ServerMsg::View(v)) => view.set(Some(v)),
                Ok(ServerMsg::Failed { error: e }) => error.set(Some(e)),
                Ok(ServerMsg::Joined { .. }) => {}
                Ok(ServerMsg::LocationTaskTemplates(templates)) => {
                    location_task_templates.set(templates);
                }
                Ok(ServerMsg::Timer(remaining)) => timer.set(remaining),
                Ok(ServerMsg::ViewedPlayer(v)) => viewed_player.set(v),
                Ok(ServerMsg::HostLoginResult { ok }) => {
                    authed.set(ok);
                    login_failed.set(!ok);
                    // Only start watching as Host -- and so only start
                    // receiving Host-privileged data at all -- once the
                    // passphrase actually checks out.
                    if ok {
                        let socket = socket;
                        spawn(async move {
                            let _ = socket.send(ClientMsg::Watch(Viewer::Host)).await;
                        });
                    }
                }
                // The connection is gone -- stop polling it. Without this,
                // a closed socket makes `recv()` return `Err` immediately
                // on every call forever, spinning this loop with no yield
                // point and pegging the tab's CPU instead of just going
                // idle. There's no reconnect story yet either way (see the
                // module doc comment), so a closed connection just stays
                // closed until the page is reloaded -- `connected` is what
                // now actually tells Dalton that's happened.
                Err(_) => {
                    connected.set(false);
                    break;
                }
            }
        }
    });

    let login = move || {
        let password = password_draft.peek().clone();
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::HostLogin { password }).await;
        });
    };

    if !authed() {
        return rsx! {
            h1 { "Host Console" }
            p { "Enter the host passphrase to continue." }
            if login_failed() {
                p { class: "error-text", "Incorrect passphrase." }
            }
            input {
                r#type: "password",
                placeholder: "Passphrase",
                value: "{password_draft}",
                oninput: move |e| password_draft.set(e.value()),
                onkeydown: move |e: Event<KeyboardData>| {
                    if e.key() == Key::Enter {
                        login();
                    }
                },
            }
            button {
                onclick: move |_| login(),
                "Log in"
            }
        };
    }

    let mut do_cmd = move |cmd: Command| {
        let socket = socket;
        error.set(None);
        spawn(async move {
            let _ = socket.send(ClientMsg::Do(cmd)).await;
        });
    };
    let mut run_raffle = move || {
        let socket = socket;
        error.set(None);
        spawn(async move {
            let _ = socket.send(ClientMsg::RunRaffle).await;
        });
    };
    let mut push_location_task = move |index: usize| {
        let socket = socket;
        error.set(None);
        spawn(async move {
            let _ = socket.send(ClientMsg::PushLocationTask { index }).await;
        });
    };
    let mut draw_intermission_entrants = move || {
        let socket = socket;
        error.set(None);
        spawn(async move {
            let _ = socket.send(ClientMsg::DrawIntermissionEntrants).await;
        });
    };
    let start_timer = move |seconds: u32| {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::StartTimer { seconds }).await;
        });
    };
    let add_timer_seconds = move |seconds: u32| {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::AddTimerSeconds { seconds }).await;
        });
    };
    let clear_timer = move || {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::ClearTimer).await;
        });
    };
    let view_player = move |id: Option<PlayerId>| {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::ViewPlayer(id)).await;
        });
    };

    // Which player's page is currently open in the read-only viewer panel
    // below -- kept alongside `viewed_player` (the actual `PlayerView` data)
    // just so the panel knows whose name to show and which `<select>` option
    // to keep highlighted; the id itself never leaves this browser tab.
    let mut viewed_player_id = use_signal(|| None::<u32>);
    // Collapsed by default -- the role reference is long (21 roles' worth
    // of flavor + ability text), and most of the Host console's other
    // panels are things Dalton needs open at a glance during live play.
    let mut show_role_reference = use_signal(|| false);
    let mut task_prompt = use_signal(String::new);
    let mut task_tier = use_signal(|| TaskTier::Easy);
    let mut task_qualifier = use_signal(|| None::<u32>);
    // The Intermission draw is once-per-game and irreversible (a misclick
    // during Round 1 permanently locks in a near-empty entrant pool for
    // the whole night) -- a review found it was a single unguarded button.
    // Same arm-then-confirm shape `AbilityPanel`'s Cult Leader Convert
    // control uses.
    let mut draw_armed = use_signal(|| false);
    let mut contest_round = use_signal(|| Round::Two);
    // Neither defaults, and both reset to `None` after every submit -- a
    // review found these stayed sticky across submissions with no reset,
    // which combined badly with `RecordContestResult` silently overwriting
    // an existing result rather than rejecting a repeat: a stale category
    // or winner selection carried into the next submit could quietly
    // clobber an already-correct result for a different category.
    let mut contest_category = use_signal(|| None::<ContestCategory>);
    let mut contest_ton_won = use_signal(|| None::<bool>);
    let mut servant_award_player = use_signal(|| None::<u32>);
    let mut servant_award_points = use_signal(|| 1u32);
    let mut gallery_winner = use_signal(|| Faction::Ton);
    // A review found the Host console never rendered Whistledown posts at
    // all (Dalton had to tab over to /play or /display to ever see one) --
    // and that even content it DOES render (the finale reveal) gives no
    // signal when it newly becomes available, just silently appearing
    // wherever its panel happens to sit on a long page. `whistledown_seen`
    // tracks how many posts Dalton has acknowledged, so a banner can show
    // exactly when there's something new.
    let mut whistledown_seen = use_signal(|| 0usize);
    // Which preset the "Start timer" button will use -- rules.md's own
    // Denouncement-phase budgets, plus a plain custom option for Round 1
    // or a contest, where there's no single rules.md number to default to.
    let mut timer_seconds = use_signal(|| 120u32);

    let roster = view().map(|v| v.roster).unwrap_or_default();
    let interest_levels = view().map(|v| v.interest_levels).unwrap_or_default();
    let raffle_closed = view().map(|v| v.raffle_closed).unwrap_or(false);
    let assigned_characters = view().map(|v| v.assigned_characters).unwrap_or_default();
    let denouncement_phase = view().and_then(|v| v.denouncement);
    let open_tasks = view().map(|v| v.open_tasks).unwrap_or_default();
    let contest_results = view().map(|v| v.contest_results).unwrap_or_default();
    let winner = view().and_then(|v| v.winner);
    let task_candidates = view().map(|v| v.task_candidates).unwrap_or_default();
    let finale_reveal = view().and_then(|v| v.finale_reveal);
    let finale_cast_out_reveal = view().map(|v| v.finale_cast_out_reveal).unwrap_or_default();
    let whistledown = view().map(|v| v.whistledown).unwrap_or_default();
    let whistledown_len = whistledown.len();
    let has_new_whistledown = whistledown_len > whistledown_seen();

    // A single always-visible "what's happening, what do I do next"
    // line -- a review found the console otherwise gives no on-page
    // signal of Denouncement sub-phase at all (Nomination vs.
    // Discussion vs. Ballot vs. Runoff), forcing a host to alt-tab to
    // `/display` before every one of the five Denouncement buttons
    // just to know which one is next.
    let phase_summary = match &view() {
        None => "Connecting...".to_string(),
        Some(v) => match &v.denouncement {
            // A review found "Start Round 1 (push tasks)" gave no
            // confirmation it worked -- a host had to switch to /display
            // to check. Folding open-task count into this same
            // always-visible banner (already used for Denouncement
            // sub-phases) covers that gap for every round, not just
            // Round 1.
            None if v.open_tasks.is_empty() => {
                format!("{:?} -- no Denouncement currently open", v.current_round)
            }
            None => format!(
                "{:?} -- no Denouncement currently open -- {} task(s) open",
                v.current_round,
                v.open_tasks.len()
            ),
            Some(DenouncementView::Nomination { .. }) => {
                format!("{:?} -- Nomination open", v.current_round)
            }
            Some(DenouncementView::Discussion { surfaced }) => format!(
                "{:?} -- Discussion (surfaced: {})",
                v.current_round,
                names(surfaced, &v.roster)
            ),
            Some(DenouncementView::Ballot { candidates, .. }) => format!(
                "{:?} -- Ballot open (candidates: {})",
                v.current_round,
                names(candidates, &v.roster)
            ),
            Some(DenouncementView::Runoff { candidates, .. }) => format!(
                "{:?} -- Runoff open (candidates: {})",
                v.current_round,
                names(candidates, &v.roster)
            ),
        },
    };

    rsx! {
        h1 { "Host Console" }
        if !connected() {
            div {
                class: "error-text",
                style: "font-weight:bold;border:2px solid;padding:0.5em;margin-bottom:0.5em;",
                "⚠ Disconnected from the server -- reload this page now. Nothing below will take effect until you do."
            }
        }
        div {
            style: "font-weight:bold;padding:0.5em 0;",
            "{phase_summary}"
        }
        if let Some(e) = error() {
            p { class: "error-text", "{e}" }
        }
        div {
            h3 { "Round timer" }
            p { "Purely a shared, visible clock -- rules.md's own phase budgets (nomination 2 min, discussion 3-4 min, ballot/resolution 90 sec) are a guide, not a rule the app enforces. It never advances anything by itself; only the buttons above/below do that." }
            TimerDisplay { remaining_secs: timer() }
            select {
                onchange: move |e| timer_seconds.set(e.value().parse().unwrap_or(120)),
                option { value: "90", "90 sec (ballot/resolution)" }
                option { value: "120", selected: true, "2 min (nomination)" }
                option { value: "240", "4 min (discussion)" }
                option { value: "300", "5 min" }
                option { value: "600", "10 min" }
            }
            button {
                onclick: move |_| start_timer(timer_seconds()),
                "Start timer"
            }
            button {
                disabled: timer().is_none(),
                onclick: move |_| add_timer_seconds(60),
                "+1 min"
            }
            button {
                disabled: timer().is_none(),
                onclick: move |_| add_timer_seconds(300),
                "+5 min"
            }
            button {
                disabled: timer().is_none(),
                onclick: move |_| clear_timer(),
                "Clear"
            }
        }
        if has_new_whistledown {
            div {
                style: "background:#4a1620;color:#f3e9d2;padding:0.75em 1em;cursor:pointer;",
                onclick: move |_| whistledown_seen.set(whistledown_len),
                "\u{1F4F0} New Whistledown post below \u{2014} tap to mark read"
            }
        }
        if finale_reveal.is_some() {
            div {
                style: "background:#4a1620;color:#f3e9d2;padding:0.75em 1em;",
                "The Finale reveal is ready \u{2014} see \"Finale reveal\" near the bottom of this page."
            }
        }
        WhistledownPosts { posts: whistledown.clone(), heading_level: 3u8 }
        div {
            h3 { "Setup" }
            p {
                "{interest_levels.len()} of {roster.len()} players have rated their interest so far (rules.md §1)."
            }
            {
                let rated: std::collections::BTreeSet<PlayerId> =
                    interest_levels.iter().map(|&(id, _)| id).collect();
                let missing: Vec<&RosterEntry> =
                    roster.iter().filter(|r| !rated.contains(&r.id)).collect();
                rsx! {
                    if !missing.is_empty() {
                        p {
                            "Still waiting on: "
                            {missing.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", ")}
                        }
                    }
                }
            }
            if raffle_closed {
                p { "Setup finalized -- roles and factions are assigned. Anyone who joins from now on becomes a Servant automatically." }
            } else {
                button {
                    onclick: move |_| run_raffle(),
                    "Finalize setup"
                }
                p {
                    "Assigns every named role by weighted ticket (higher interest = more tickets), splits everyone else across Ton/Uprising, and starts Round 1 -- anyone who joins from now on becomes a Servant automatically."
                }
            }
        }
        div {
            h3 { "Role reference" }
            p { "Every role in the game, assigned or not, with its flavor text and ability -- doesn't say who holds an assigned role, just that someone does." }
            button {
                onclick: move |_| show_role_reference.set(!show_role_reference()),
                if show_role_reference() { "Hide role reference" } else { "Show role reference" }
            }
            if show_role_reference() {
                RoleReference { assigned: assigned_characters.clone() }
            }
        }
        div {
            h3 { "Round" }
            if let Some(v) = view() {
                p { "Current round: {v.current_round:?}" }
            }
            button { onclick: move |_| do_cmd(Command::AdvanceRound), "Advance round" }
        }
        div {
            h3 { "The Denouncement" }
            p { "Only the button matching the current phase above is enabled. Nomination, the ballot, and any runoff also close themselves automatically the instant everyone who can act has -- no need to wait out the full time budget or watch the room for stragglers. Discussion still needs your own judgment call, so \"Open ballot\" stays a manual click." }
            if denouncement_phase.is_none() && !open_tasks.is_empty() {
                p { class: "warning-text",
                    "{open_tasks.len()} task(s) are still open -- rules.md locks submissions before nomination starts. Close tasks below first, or open the Denouncement anyway if that's intentional."
                }
            }
            button {
                disabled: denouncement_phase.is_some(),
                onclick: move |_| do_cmd(Command::OpenDenouncement),
                "Open Denouncement",
            }
            button {
                disabled: !matches!(denouncement_phase, Some(DenouncementView::Nomination { .. })),
                onclick: move |_| do_cmd(Command::CloseNomination),
                "Close nomination",
            }
            button {
                disabled: !matches!(denouncement_phase, Some(DenouncementView::Discussion { .. })),
                onclick: move |_| do_cmd(Command::OpenBallot),
                "Open ballot",
            }
            button {
                disabled: !matches!(denouncement_phase, Some(DenouncementView::Ballot { .. })),
                onclick: move |_| do_cmd(Command::CloseBallot { fallback_replacement: None }),
                "Close ballot"
            }
            button {
                disabled: !matches!(denouncement_phase, Some(DenouncementView::Runoff { .. })),
                onclick: move |_| do_cmd(Command::CloseRunoff { fallback_replacement: None }),
                "Close runoff"
            }
        }
        div {
            h3 { "Tasks" }
            p { "Every round's tasks push automatically -- Round 1's the moment setup finalizes, Rounds 3 and 5 the moment that round's task phase begins. No action needed here." }
            if open_tasks.is_empty() {
                p { "No tasks currently open." }
            } else {
                p { "Currently open ({open_tasks.len()}):" }
                ul {
                    for task in open_tasks.clone() {
                        li { key: "{task.id.0}", "{task.prompt} ({task.tier:?})" }
                    }
                }
            }
            button { onclick: move |_| do_cmd(Command::CloseTasks), "Close tasks" }
            p { "The controls below are for a manual top-up or fix-up only." }
            h4 { "From player bios (rules.md §4, Rounds 3/5)" }
            for (tier , candidates) in task_candidates.clone() {
                div {
                    key: "{tier:?}",
                    p { "{tier:?} ({candidates.len()} candidates):" }
                    for candidate in candidates.into_iter().take(8) {
                        button {
                            key: "{candidate.prompt}",
                            onclick: {
                                let candidate = candidate.clone();
                                move |_| {
                                    do_cmd(Command::PushTask {
                                        prompt: candidate.prompt.clone(),
                                        tier,
                                        qualifying_players: candidate.qualifying_players.iter().copied().collect(),
                                        expected_code: None,
                                    });
                                }
                            },
                            "{candidate.prompt}"
                        }
                    }
                }
            }
            h4 { "Location tasks (rules.md §4: talk to someone at a place)" }
            p { "Medium states the location plainly; hard is a riddle -- both are credited once the player enters the code you've physically placed there." }
            for (index , tier , prompt) in location_task_templates() {
                button {
                    key: "{index}",
                    onclick: move |_| push_location_task(index),
                    "[{tier:?}] {prompt}"
                }
            }
            h4 { "Manual entry (Round 1's fixed tasks, or a fallback)" }
            input {
                placeholder: "Task prompt",
                value: "{task_prompt}",
                oninput: move |e| task_prompt.set(e.value()),
            }
            select {
                onchange: move |e| {
                    task_tier.set(match e.value().as_str() {
                        "Medium" => TaskTier::Medium,
                        "Hard" => TaskTier::Hard,
                        _ => TaskTier::Easy,
                    });
                },
                option { value: "Easy", "Easy" }
                option { value: "Medium", "Medium" }
                option { value: "Hard", "Hard" }
            }
            PlayerSelect {
                roster: roster.clone(),
                placeholder: "-- who qualifies? --",
                on_change: move |e: FormEvent| task_qualifier.set(e.value().parse().ok()),
            }
            button {
                disabled: task_prompt().trim().is_empty() || task_qualifier().is_none(),
                onclick: move |_| {
                    let prompt = task_prompt.peek().trim().to_string();
                    let Some(qualifier) = task_qualifier() else { return };
                    do_cmd(Command::PushTask {
                        prompt,
                        tier: task_tier(),
                        qualifying_players: [PlayerId(qualifier)].into_iter().collect(),
                        expected_code: None,
                    });
                    task_prompt.set(String::new());
                },
                "Push task"
            }
        }
        div {
            h3 { "Contest rounds (Round 2 & 4)" }
            p { "The actual mini-games are designed later -- this just records each category's result. Players never see the running standings or the breakdown (only the engine tracks it, for the Leader's Confidants)." }
            p {
                if contest_round() == Round::Two {
                    "Round 2: the whole room competes together, one category at a time."
                } else {
                    "Round 4: 3 simultaneous zones, one per category -- a Cast-Out scorekeeper reports each zone's result in as it finishes."
                }
            }
            select {
                onchange: move |e| {
                    contest_round.set(if e.value() == "Four" { Round::Four } else { Round::Two });
                },
                option { value: "Two", "Round 2" }
                option { value: "Four", "Round 4" }
            }
            select {
                value: match contest_category() {
                    Some(ContestCategory::Strength) => "Strength",
                    Some(ContestCategory::Creativity) => "Creativity",
                    Some(ContestCategory::Intelligence) => "Intelligence",
                    None => "",
                },
                onchange: move |e| {
                    contest_category.set(match e.value().as_str() {
                        "Strength" => Some(ContestCategory::Strength),
                        "Creativity" => Some(ContestCategory::Creativity),
                        "Intelligence" => Some(ContestCategory::Intelligence),
                        _ => None,
                    });
                },
                option { value: "", "-- category --" }
                option { value: "Strength", "Strength" }
                option { value: "Creativity", "Creativity" }
                option { value: "Intelligence", "Intelligence" }
            }
            select {
                value: match contest_ton_won() {
                    Some(true) => "Ton",
                    Some(false) => "Uprising",
                    None => "",
                },
                onchange: move |e| {
                    contest_ton_won.set(match e.value().as_str() {
                        "Ton" => Some(true),
                        "Uprising" => Some(false),
                        _ => None,
                    });
                },
                option { value: "", "-- who won? --" }
                option { value: "Ton", "Ton won" }
                option { value: "Uprising", "Uprising won" }
            }
            button {
                disabled: contest_category().is_none() || contest_ton_won().is_none(),
                onclick: move |_| {
                    let (Some(category), Some(ton_won)) = (contest_category(), contest_ton_won())
                    else {
                        return;
                    };
                    do_cmd(Command::RecordContestResult { round: contest_round(), category, ton_won });
                    contest_category.set(None);
                    contest_ton_won.set(None);
                },
                "Record result"
            }
            if contest_results.is_empty() {
                p { "Nothing recorded yet." }
            } else {
                p { "Already recorded (host-only self-audit -- players never see this):" }
                ul {
                    for ((round , category) , ton_won) in contest_results.clone() {
                        {
                            let winner = if ton_won { "Ton" } else { "Uprising" };
                            rsx! {
                                li {
                                    key: "{round:?}-{category:?}",
                                    "{round:?} / {category:?}: {winner} won"
                                }
                            }
                        }
                    }
                }
            }
        }
        div {
            h3 { "Intermission lottery" }
            p { "Draws up to 5 entrants from whoever opted in and is still active -- you don't get a names-and-opt-ins list (same no-ambient-god-view rule as everywhere else), so this runs the draw server-side instead of asking you to pick. This can only run once per game, so double-check everyone who wants in has opted in before confirming." }
            button {
                onclick: move |_| {
                    if !draw_armed() {
                        draw_armed.set(true);
                        return;
                    }
                    draw_armed.set(false);
                    draw_intermission_entrants();
                },
                if draw_armed() { "Confirm draw -- click again" } else { "Draw entrants" }
            }
        }
        div {
            h3 { "Servant leaderboard" }
            p { "Any Servant, or any already-Cast-Out player, is eligible. What earns points (zone scorekeeping, trivia, a minigame) is up to you -- the app just tracks the running total." }
            PlayerSelect {
                roster: roster.clone(),
                placeholder: "-- player --",
                on_change: move |e: FormEvent| servant_award_player.set(e.value().parse().ok()),
            }
            input {
                r#type: "number",
                min: "1",
                value: "{servant_award_points}",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse() {
                        servant_award_points.set(v);
                    }
                },
            }
            button {
                disabled: servant_award_player().is_none(),
                onclick: move |_| {
                    let Some(player) = servant_award_player() else { return };
                    do_cmd(Command::AwardServantPoints {
                        player: PlayerId(player),
                        points: servant_award_points(),
                    });
                },
                "Award points"
            }
        }
        div {
            h3 { "Gallery resolution" }
            p { "Once at the Finale, after the ballot has actually closed: score every submitted Gallery prediction against the real outcome. Only one faction ever wins -- the Cult has priority over any overlap (see win_condition::evaluate's doc comment)." }
            // Both inputs below come straight from state the engine already
            // computed (`finale_cast_out_reveal`, `winner`) -- a review
            // found this panel used to make Dalton retype Cast-Out player
            // IDs by hand from memory, redundant with (and a real risk of
            // drifting from) the public reveal the app already shows him.
            p {
                "Cast Out at the Last Denouncement: "
                if finale_cast_out_reveal.is_empty() {
                    "(none yet -- the ballot hasn't closed)"
                } else {
                    "{names(&finale_cast_out_reveal.iter().map(|p| p.id).collect::<Vec<_>>(), &roster)}"
                }
            }
            if let Some(f) = winner {
                // The button sends the engine's own computed answer
                // directly -- never a separately-tracked signal that could
                // silently drift out of sync with it and mis-score every
                // Gallery prediction, irreversibly, at the Finale.
                p { "The engine computed the winner as {f:?} -- this is what gets recorded." }
                button {
                    onclick: move |_| {
                        let actual_cast_out = finale_cast_out_reveal.iter().map(|p| p.id).collect();
                        do_cmd(Command::ResolveGalleryPredictions {
                            actual_cast_out,
                            actual_winner: f,
                        });
                    },
                    "Resolve Gallery ({f:?} wins)"
                }
            } else {
                p { "The engine hasn't computed a winner -- either the Finale's Denouncement hasn't closed yet, or this is the rare legitimate case where nobody's win condition was met (see win_condition::evaluate's doc comment). Pick a winner to record manually:" }
                for faction in [Faction::Ton, Faction::Uprising, Faction::Cult] {
                    label {
                        input {
                            r#type: "radio",
                            name: "gallery-winner",
                            checked: gallery_winner() == faction,
                            onchange: move |_| gallery_winner.set(faction),
                        }
                        " {faction:?} won"
                    }
                }
                button {
                    onclick: move |_| {
                        let actual_cast_out = finale_cast_out_reveal.iter().map(|p| p.id).collect();
                        do_cmd(Command::ResolveGalleryPredictions {
                            actual_cast_out,
                            actual_winner: gallery_winner(),
                        });
                    },
                    "Resolve Gallery (manual override)"
                }
            }
        }
        if let Some(r) = finale_reveal {
            div {
                h3 { "Finale reveal (for Dalton to narrate)" }
                p { "rules.md §4: \"Dalton walks through all three win conditions and reveals everything that happened privately all game.\"" }
                p {
                    strong {
                        if r.winner.cult_wins { "The Cult wins." }
                        else if r.winner.ton_wins { "The Ton wins." }
                        else if r.winner.uprising_wins { "The Uprising wins." }
                        else { "Nobody's win condition was met." }
                    }
                    if !r.winner.cult_paths.is_empty() {
                        " (Cult paths satisfied: {r.winner.cult_paths:?})"
                    }
                }
                h4 { "Everyone's true identity" }
                ul {
                    for p in r.everyone.clone() {
                        li {
                            key: "{p.id.0}",
                            "{p.name}: {p.true_faction:?}"
                            if let Some(c) = p.character { ", {c:?}" }
                            if p.converted { " (converted)" }
                        }
                    }
                }
                h4 { "What happened privately" }
                ul {
                    for (i , event) in r.key_events.iter().enumerate() {
                        li { key: "{i}", "{describe_key_event(event, &r.everyone)}" }
                    }
                }
            }
        }
        div {
            h3 { "View a player's page" }
            p { "Read-only -- shows exactly what that player currently sees on their own phone right now. There are no buttons or forms here, so nothing in this panel can act on their behalf." }
            PlayerSelect {
                roster: roster.clone(),
                placeholder: "-- pick a player --",
                on_change: move |e: FormEvent| {
                    let id: Option<u32> = e.value().parse().ok();
                    viewed_player_id.set(id);
                    // Cleared immediately rather than left stale until the
                    // server replies -- otherwise a fast re-pick could
                    // briefly show the new player's name over the
                    // previous player's still-cached data.
                    viewed_player.set(None);
                    view_player(id.map(PlayerId));
                },
            }
            if let Some(id) = viewed_player_id() {
                if let Some(v) = viewed_player() {
                    PlayerPageReadOnly { id: PlayerId(id), v }
                } else {
                    p { "Loading..." }
                }
                button {
                    onclick: move |_| {
                        viewed_player_id.set(None);
                        view_player(None);
                    },
                    "Close",
                }
            }
        }
        RosterList { roster }
    }
}

/// Plain-English narration for `Host`'s Finale reveal panel -- the same
/// spirit as `describe_check` elsewhere in this file, translating a raw
/// `DomainEvent` into something Dalton can actually read aloud.
fn describe_key_event(event: &DomainEvent, everyone: &[engine::PlayerReveal]) -> String {
    let name = |id: PlayerId| {
        everyone
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("player {}", id.0))
    };
    match event {
        DomainEvent::Converted { converter, target } => {
            format!(
                "{} converted {} to the Cult.",
                name(*converter),
                name(*target)
            )
        }
        DomainEvent::KingQueenConversionCascade {
            old_king_queen,
            new_king_queen,
            ..
        } => {
            format!(
                "King/Queen {} was converted; the crown passed to {}.",
                name(*old_king_queen),
                new_king_queen
                    .map(name)
                    .unwrap_or_else(|| "no one -- none remained".into())
            )
        }
        DomainEvent::KingQueenCastOutCascade {
            old_king_queen,
            new_king_queen,
            ..
        } => {
            format!(
                "King/Queen {} was Cast Out; the crown passed to {}.",
                name(*old_king_queen),
                new_king_queen
                    .map(name)
                    .unwrap_or_else(|| "no one -- none remained".into())
            )
        }
        DomainEvent::RevolutionaryLeaderSucceeded {
            old_leader,
            new_leader,
        } => {
            format!(
                "Revolutionary Leader {} fell; the title passed to {}.",
                name(*old_leader),
                new_leader
                    .map(name)
                    .unwrap_or_else(|| "no one -- the line was exhausted".into())
            )
        }
        DomainEvent::MartyrdomTriggered { cult_leader } => {
            format!(
                "Cult Leader {} was Cast Out, triggering martyrdom.",
                name(*cult_leader)
            )
        }
        other => format!("{other:?}"),
    }
}

// --- /display ----------------------------------------------------------------

#[component]
fn Display() -> Element {
    let mut view = use_signal(|| None::<PlayerView>);
    let mut timer = use_signal(|| None::<i64>);
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        let _ = socket.send(ClientMsg::Watch(Viewer::Display)).await;
        loop {
            match socket.recv().await {
                Ok(ServerMsg::View(v)) => view.set(Some(v)),
                Ok(ServerMsg::Timer(remaining)) => timer.set(remaining),
                Ok(
                    ServerMsg::Failed { .. }
                    | ServerMsg::Joined { .. }
                    | ServerMsg::LocationTaskTemplates(_)
                    | ServerMsg::HostLoginResult { .. }
                    | ServerMsg::ViewedPlayer(_),
                ) => {}
                // See the identical comment in `Host` -- without this, a
                // closed connection spins this loop forever with no yield.
                Err(_) => break,
            }
        }
    });

    let Some(v) = view() else {
        return rsx! {
            div { class: "route-display", h1 { "Murder Mystery 2026" } }
        };
    };

    rsx! {
        div { class: "route-display",
            h1 { "Murder Mystery 2026" }
            h2 { "Round: {v.current_round:?}" }
            TimerDisplay { remaining_secs: timer() }
            RosterList { roster: v.roster.clone() }
            match &v.denouncement {
                Some(DenouncementView::Nomination { .. }) => rsx! { p { "Nomination is open." } },
                Some(DenouncementView::Discussion { surfaced }) => rsx! {
                    p { "Up for the Denouncement: {names(surfaced, &v.roster)}" }
                },
                Some(DenouncementView::Ballot { candidates, .. }) => rsx! {
                    p { "Ballot open for: {names(candidates, &v.roster)}" }
                },
                Some(DenouncementView::Runoff { candidates, .. }) => rsx! {
                    p { "Runoff -- the first ballot tied. Voting again for: {names(candidates, &v.roster)}" }
                },
                None => rsx! { p { "No Denouncement in progress." } },
            }
            for task in v.open_tasks.iter().cloned() {
                p { key: "{task.id.0}", "{task.prompt} ({task.tier:?})" }
            }
            FinaleCastOutReveal { reveal: v.finale_cast_out_reveal.clone(), heading_level: 2u8 }
            WhistledownPosts { posts: v.whistledown.clone(), heading_level: 2u8 }
        }
    }
}
