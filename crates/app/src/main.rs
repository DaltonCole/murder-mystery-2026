//! The three role-based routes from the implementation plan (`/`=play,
//! `/host`, `/display`), talking to the single game server over one typed
//! websocket connection (see `game_server` for the server-side state and
//! `ClientMsg`/`ServerMsg` below for the wire protocol).
//!
//! Phase 1 scope: these are genuinely basic shells -- functional enough for
//! a real LAN playtest, not the final visual design (that's Phase 4). See
//! `/home/drc/.claude/plans/piped-crunching-lighthouse.md`.
//!
//! KNOWN GAP, not an oversight: there is no session/auth layer yet, and
//! it's broader than just view privacy. `game_ws` accepts any `ClientMsg`
//! from any connection with no identity check at all:
//! - `Watch(Viewer::Player(id))` lets a crafted client read any player's
//!   private view. This UI never sends that itself -- a fresh `/play`
//!   connection only ever watches the id its own `Join` call just
//!   received -- but nothing stops a deliberately crafted client from
//!   doing so.
//! - `Do(Command)` goes further: since commands like `CastBallot`,
//!   `Nominate`, and `AttemptTask` carry the acting player's id as a plain
//!   field with nothing tying it to the sending connection, any client can
//!   impersonate *any* player's writes, not just reads -- vote as someone
//!   else, submit fake task attempts.
//! - Every host-only command (`AddPlayer`, `FinalizeSetup`,
//!   `AdvanceRound`, `OpenDenouncement`, `CloseNomination`, `OpenBallot`,
//!   `CloseBallot`, `CloseRunoff`, `PushTask`, `CloseTasks`) can be issued
//!   from a raw connection to `/api/ws` regardless of which route it came
//!   through -- nothing distinguishes a Host console's socket from a
//!   Player's. The same is true of the two host-only `ClientMsg` variants
//!   that aren't plain `Command`s either (`RunRaffle`, `PushLocationTask`).
//!
//! Real per-player join tokens and a real Host credential (see the plan's
//! "Session" section) must land before this runs at a real event over
//! shared WiFi.
//!
//! SECOND KNOWN GAP: no reconnect story. Every route's `use_websocket` call
//! uses a plain `WebSocketOptions::new()`, not
//! `.with_automatic_reconnect()`; once a connection drops (a WiFi hiccup,
//! laptop sleep, a `dx serve` restart), that tab just goes quiet with no
//! user-visible indicator -- the only recovery is a manual page reload.
//! Deliberately not wiring up automatic reconnect in this pass: doing so
//! safely also requires re-verifying the receive-loop's error handling
//! (see the `Err(_) => break` comments in `Play`/`Host`/`Display` below)
//! against real reconnect behavior in an actual browser, which this
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
    Viewer, WhistledownPost,
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
    /// `/host` only: pushes Round 1's randomly-selected tasks on demand.
    /// Not a plain `Command` -- needs the same server-side real RNG as
    /// `RunRaffle`. See `game_server::start_round_one`'s doc comment for
    /// why this is a deliberate, separate Host trigger rather than firing
    /// automatically the instant setup finalizes.
    StartRoundOne,
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
}

#[get("/api/ws")]
async fn game_ws(options: WebSocketOptions) -> Result<Websocket<ClientMsg, ServerMsg>> {
    Ok(options.on_upgrade(move |mut socket| async move {
        let mut viewer: Option<Viewer> = None;
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
                                    drain_self_echo(&mut changed);
                                    socket.send(ServerMsg::Joined { player: id }).await.is_ok()
                                        && socket
                                            .send(ServerMsg::View(game_server::view(Viewer::Player(id))))
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
                            sent_view && sent_templates
                        }
                        ClientMsg::Do(cmd) => respond!(game_server::apply(cmd)),
                        ClientMsg::RunRaffle => respond!(game_server::run_raffle()),
                        ClientMsg::PushLocationTask { index } => {
                            respond!(game_server::push_location_task(index))
                        }
                        ClientMsg::DrawIntermissionEntrants => {
                            respond!(game_server::draw_intermission_entrants())
                        }
                        ClientMsg::StartRoundOne => respond!(game_server::start_round_one()),
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
                }
            }
        }
    }))
}

// --- /play -----------------------------------------------------------------

#[component]
fn Play() -> Element {
    let mut view = use_signal(|| None::<PlayerView>);
    let mut my_id = use_signal(|| None::<PlayerId>);
    let mut error = use_signal(|| None::<String>);
    let mut name_draft = use_signal(String::new);
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        loop {
            match socket.recv().await {
                Ok(ServerMsg::Joined { player }) => my_id.set(Some(player)),
                Ok(ServerMsg::View(v)) => {
                    view.set(Some(v));
                    error.set(None);
                }
                Ok(ServerMsg::Failed { error: e }) => error.set(Some(e)),
                // `/play` never watches as `Viewer::Host`, so this never
                // actually arrives here -- see `ServerMsg::LocationTaskTemplates`'s
                // doc comment.
                Ok(ServerMsg::LocationTaskTemplates(_)) => {}
                Err(_) => break,
            }
        }
    });

    let send_cmd = move |cmd: Command| {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::Do(cmd)).await;
        });
    };

    let mut gallery_pick = use_signal(|| None::<u32>);
    let mut gallery_faction_pick = use_signal(|| None::<Faction>);

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
                p { style: "color:red", "{e}" }
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

    rsx! {
        h1 { "Murder Mystery 2026" }
        if let Some(e) = error() {
            p { style: "color:red", "{e}" }
        }
        p { "Round: {v.current_round:?}" }
        p {
            "Your faction: {v.own_faction:?}"
            if let Some(c) = v.own_character {
                " -- {c:?}"
            }
        }
        if v.i_am_drunk {
            p { style: "color:red", "You're drunk this round -- you can't nominate or vote." }
        }
        if let Some(leader) = v.revealed_leader {
            p { "You now know the Revolutionary Leader: {names(&[leader], &v.roster)}." }
        }
        if !v.my_confidants.is_empty() {
            p { "These people now know you're the Revolutionary Leader: {names(&v.my_confidants, &v.roster)}." }
        }
        if let Some(message) = &v.martyrdom_message {
            p { class: "martyrdom-message", "{message}" }
        }
        FinaleCastOutReveal { reveal: v.finale_cast_out_reveal.clone(), heading_level: 4u8 }
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
        if v.roster.iter().any(|r| r.id == id && r.status == PlayerStatus::CastOut) {
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
        if v.own_character.is_none() {
            InterestLevelForm {
                my_id: id,
                own_interest_level: v.own_interest_level,
                on_command: send_cmd,
            }
        }
        BioForm {
            my_id: id,
            own_bio: v.own_bio.clone(),
            on_command: send_cmd,
        }
        RosterList { roster: v.roster.clone() }
        AbilityPanel {
            my_id: id,
            own_character: v.own_character,
            abilities: v.my_abilities.clone(),
            my_info_checks: v.my_info_checks.clone(),
            fellow_cultists: v.fellow_cultists.clone(),
            known_uprising_members: v.known_uprising_members.clone(),
            roster: v.roster.clone(),
            on_command: send_cmd,
        }
        DenouncementPanel {
            my_id: id,
            denouncement: v.denouncement.clone(),
            roster: v.roster.clone(),
            on_command: send_cmd,
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
    }
}

/// rules.md §1's signup interest rating -- only shown before the setup
/// raffle has given the viewer a character (`own_character` still `None`
/// in `Play`'s caller); once it has, there's nothing left to rate. A
/// standing choice like `SubmitBio` -- resubmitting silently replaces (see
/// `Command::SubmitInterestLevel`'s doc comment), so this doesn't need a
/// separate "already submitted, lock it in" state.
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
                "Rate your interest 1-10 -- a higher rating gives you more tickets in the raffle for a major role (rules.md §1). The host runs the raffle once everyone's rated themselves."
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
                p { "You rated your interest: {submitted}. Waiting for the host to run the raffle." }
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
/// player id -- the Host console's recurring "pick a player" control (6
/// call sites: faction/character assignment, a task's qualifying player,
/// Convert's converter/target, a Servant point award). `on_change` gets the
/// raw `FormEvent` rather than an already-parsed `PlayerId` so each call
/// site keeps full control of what else its change should do (a couple
/// also reset an unrelated "armed" confirmation state).
#[component]
fn PlayerSelect(
    roster: Vec<RosterEntry>,
    placeholder: &'static str,
    on_change: EventHandler<FormEvent>,
) -> Element {
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
                    if let Some(c) = p.character { ", {c:?}" }
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

/// rules.md §1's "Character creation" -- free text, each field capped at
/// 32 characters (enforced here via `maxlength` and, as the real source of
/// truth, by the engine's own `SubmitBio` validation). Doesn't pre-fill
/// from an already-submitted `own_bio` -- resubmitting silently replaces
/// (see `Command::SubmitBio`'s doc comment), so editing just means
/// retyping, matching this Phase 1-era "basic shell" UI's overall level of
/// polish elsewhere.
#[component]
fn BioForm(my_id: PlayerId, own_bio: Option<Bio>, on_command: EventHandler<Command>) -> Element {
    let mut character_name = use_signal(String::new);
    let mut real_name = use_signal(String::new);
    let mut occupation = use_signal(String::new);
    let mut hobbies = use_signal(|| std::array::from_fn::<String, 5, _>(|_| String::new()));
    let mut clothing_features =
        use_signal(|| std::array::from_fn::<String, 5, _>(|_| String::new()));
    let mut skills = use_signal(|| std::array::from_fn::<String, 5, _>(|_| String::new()));

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
            input {
                placeholder: "Character name",
                maxlength: "32",
                value: "{character_name}",
                oninput: move |e| character_name.set(e.value()),
            }
            input {
                placeholder: "Real name",
                maxlength: "32",
                value: "{real_name}",
                oninput: move |e| real_name.set(e.value()),
            }
            input {
                placeholder: "Occupation",
                maxlength: "32",
                value: "{occupation}",
                oninput: move |e| occupation.set(e.value()),
            }
            p { "Hobbies (up to 5):" }
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
            p { "Notable clothing features (up to 5):" }
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
            p { "Skills (up to 5):" }
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
            button {
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

/// Phase 2 scope: one raw-controls panel covering every ability-bearing
/// character, matching the rest of this Phase 1-era "basic shell" UI --
/// see the module doc comment. Bartender's "did it land" is a checkbox the
/// player sets from an actual coin flip at the table rather than the app
/// rolling it itself, the same "keep randomness at the boundary, let a
/// human adjudicate it" choice the Host's Convert panel already makes for
/// `CastOut`'s `fallback_replacement`.
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
) -> Element {
    let Some(character) = own_character else {
        return rsx! {};
    };

    let mut target = use_signal(|| None::<u32>);
    let mut lands = use_signal(|| true);
    let mut kind = use_signal(|| InfoQueryKind::IsTheLeader);

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

    rsx! {
        div {
            h3 { "Your ability" }
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
                    label {
                        input {
                            r#type: "checkbox",
                            checked: lands(),
                            onchange: move |e| lands.set(e.checked()),
                        }
                        " it lands (flip a coin at the table)"
                    }
                    button {
                        disabled: !abilities.bartender_available.unwrap_or(false) || target().is_none(),
                        onclick: move |_| {
                            let Some(t) = target() else { return };
                            on_command
                                .call(Command::BartenderTarget {
                                    player: my_id,
                                    target: PlayerId(t),
                                    lands: lands(),
                                });
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
        InfoCheckAnswer::Dossier(d) => format!(
            "{target}: apparent faction {:?}, converted: {}, character: {:?}",
            d.apparent_faction, d.converted, d.character
        ),
        InfoCheckAnswer::Faction(f) => format!("{target}: faction color {f:?}"),
        // `kind` distinguishes the Cult Leader's two possible yes/no
        // queries (IsTheLeader vs. IsTonAligned) -- without it, two
        // queries against different players/questions would render as
        // indistinguishable "Name: true/false" lines.
        InfoCheckAnswer::Bool(b) => format!("{target} ({:?}): {b}", check.kind),
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

    let active_others: Vec<RosterEntry> = roster
        .into_iter()
        .filter(|r| r.status == PlayerStatus::Active && r.id != my_id)
        .collect();

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
                h3 { "Ballot" }
                p { if i_have_acted { "You've voted." } else { "Cast your ballot." } }
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

#[component]
fn TaskAttemptForm(
    my_id: PlayerId,
    task: TaskView,
    roster: Vec<RosterEntry>,
    on_command: EventHandler<Command>,
) -> Element {
    let options: Vec<RosterEntry> = roster
        .into_iter()
        .filter(|r| r.status == PlayerStatus::Active && r.id != my_id)
        .collect();

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
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        let _ = socket.send(ClientMsg::Watch(Viewer::Host)).await;
        loop {
            match socket.recv().await {
                Ok(ServerMsg::View(v)) => {
                    view.set(Some(v));
                    error.set(None);
                }
                Ok(ServerMsg::Failed { error: e }) => error.set(Some(e)),
                Ok(ServerMsg::Joined { .. }) => {}
                Ok(ServerMsg::LocationTaskTemplates(templates)) => {
                    location_task_templates.set(templates);
                }
                // The connection is gone -- stop polling it. Without this,
                // a closed socket makes `recv()` return `Err` immediately
                // on every call forever, spinning this loop with no yield
                // point and pegging the tab's CPU instead of just going
                // idle. There's no reconnect story yet either way (see the
                // module doc comment), so a closed connection just stays
                // closed until the page is reloaded.
                Err(_) => break,
            }
        }
    });

    let do_cmd = move |cmd: Command| {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::Do(cmd)).await;
        });
    };
    let run_raffle = move || {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::RunRaffle).await;
        });
    };
    let push_location_task = move |index: usize| {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::PushLocationTask { index }).await;
        });
    };
    let draw_intermission_entrants = move || {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::DrawIntermissionEntrants).await;
        });
    };
    let start_round_one = move || {
        let socket = socket;
        spawn(async move {
            let _ = socket.send(ClientMsg::StartRoundOne).await;
        });
    };

    let mut new_name = use_signal(String::new);
    let mut faction_player = use_signal(|| None::<u32>);
    let mut faction_choice = use_signal(|| Faction::Ton);
    let mut character_player = use_signal(|| None::<u32>);
    let mut character_choice = use_signal(|| Character::KingQueen);
    let mut task_prompt = use_signal(String::new);
    let mut task_tier = use_signal(|| TaskTier::Easy);
    let mut task_qualifier = use_signal(|| None::<u32>);
    let mut convert_converter = use_signal(|| None::<u32>);
    let mut convert_target = use_signal(|| None::<u32>);
    // A review flagged this panel as unguarded, catastrophic-if-misclicked
    // dev tooling -- but it's actually the *only* way `Command::Convert`
    // is reachable at all (the Cult Leader has no self-service UI for it
    // via `/play`; they tell the Host who to convert, and the Host acts on
    // it here), so hiding it would break the Cult's core recruitment
    // mechanic. The real fix is a confirm step, not removal: `convert_armed`
    // requires a second, distinct click before the command actually fires,
    // and resets the moment either dropdown changes.
    let mut convert_armed = use_signal(|| false);
    let mut contest_round = use_signal(|| Round::Two);
    let mut contest_category = use_signal(|| ContestCategory::Strength);
    let mut contest_ton_won = use_signal(|| true);
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

    let roster = view().map(|v| v.roster).unwrap_or_default();
    let interest_levels = view().map(|v| v.interest_levels).unwrap_or_default();
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
            None => format!("{:?} -- no Denouncement currently open", v.current_round),
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
        div {
            style: "font-weight:bold;padding:0.5em 0;",
            "{phase_summary}"
        }
        if let Some(e) = error() {
            p { style: "color:red", "{e}" }
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
            input {
                placeholder: "New player name",
                value: "{new_name}",
                oninput: move |e| new_name.set(e.value()),
            }
            button {
                onclick: move |_| {
                    let name = new_name.peek().trim().to_string();
                    if !name.is_empty() {
                        do_cmd(Command::AddPlayer { name });
                        new_name.set(String::new());
                    }
                },
                "Add player"
            }
            p {
                "{interest_levels.len()} of {roster.len()} players have rated their interest so far (rules.md §1)."
            }
            button {
                onclick: move |_| run_raffle(),
                "Run the raffle"
            }
            p {
                "Running the raffle assigns every named role by weighted ticket (higher interest = more tickets), splits everyone else across Ton/Uprising, then finalizes setup -- anyone added afterward joins as a Servant automatically. The controls below are for a manual fix-up afterward, or for designating the Deceiver mid-game."
            }
            div {
                PlayerSelect {
                    roster: roster.clone(),
                    placeholder: "-- player --",
                    on_change: move |e: FormEvent| faction_player.set(e.value().parse().ok()),
                }
                select {
                    onchange: move |e| {
                        faction_choice.set(match e.value().as_str() {
                            "Uprising" => Faction::Uprising,
                            "Cult" => Faction::Cult,
                            "Servant" => Faction::Servant,
                            _ => Faction::Ton,
                        });
                    },
                    option { value: "Ton", "Ton" }
                    option { value: "Uprising", "Uprising" }
                    option { value: "Cult", "Cult" }
                    option { value: "Servant", "Servant" }
                }
                button {
                    disabled: faction_player().is_none(),
                    onclick: move |_| {
                        let Some(player) = faction_player() else { return };
                        do_cmd(Command::AssignFaction {
                            player: PlayerId(player),
                            faction: faction_choice(),
                        });
                    },
                    "Assign faction"
                }
            }
            div {
                PlayerSelect {
                    roster: roster.clone(),
                    placeholder: "-- player --",
                    on_change: move |e: FormEvent| character_player.set(e.value().parse().ok()),
                }
                select {
                    onchange: move |e| {
                        character_choice.set(match e.value().as_str() {
                            "PrincePrincess" => Character::PrincePrincess,
                            "RevolutionaryLeader" => Character::RevolutionaryLeader,
                            "CultLeader" => Character::CultLeader,
                            _ => Character::KingQueen,
                        });
                    },
                    option { value: "KingQueen", "King/Queen" }
                    option { value: "PrincePrincess", "Prince/Princess" }
                    option { value: "RevolutionaryLeader", "Revolutionary Leader" }
                    option { value: "CultLeader", "Cult Leader" }
                }
                button {
                    disabled: character_player().is_none(),
                    onclick: move |_| {
                        let Some(player) = character_player() else { return };
                        do_cmd(Command::AssignCharacter {
                            player: PlayerId(player),
                            character: character_choice(),
                        });
                    },
                    "Assign title"
                }
            }
            p { "Manual path (skip this if you used \"Run the raffle\" above): assign a faction to every player first, assign the four titles above to their holders, then Finalize -- everyone else gets a generic character automatically." }
            button { onclick: move |_| do_cmd(Command::FinalizeSetup), "Finalize setup" }
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
            button { onclick: move |_| do_cmd(Command::OpenDenouncement), "Open Denouncement" }
            button { onclick: move |_| do_cmd(Command::CloseNomination), "Close nomination" }
            button { onclick: move |_| do_cmd(Command::OpenBallot), "Open ballot" }
            button {
                onclick: move |_| do_cmd(Command::CloseBallot { fallback_replacement: None }),
                "Close ballot"
            }
            button {
                onclick: move |_| do_cmd(Command::CloseRunoff { fallback_replacement: None }),
                "Close runoff"
            }
        }
        div {
            h3 { "Tasks" }
            p { "Rounds 3 and 5 each auto-push their own bio-derived tasks the moment that round's task phase begins -- no action needed there. Round 1's two tasks wait for you: click below once you've given the live intro and everyone's revealed their character." }
            button { onclick: move |_| start_round_one(), "Start Round 1 (push tasks)" }
            p { "The controls below are for a manual top-up or fix-up only." }
            h4 { "From player bios (rules.md §4, Rounds 3/5)" }
            for (tier , candidates) in task_candidates.clone() {
                div {
                    key: "{tier:?}",
                    p { "{tier:?}:" }
                    for candidate in candidates {
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
            button { onclick: move |_| do_cmd(Command::CloseTasks), "Close tasks" }
        }
        div {
            h3 { "Cult Leader: Convert" }
            p { "The Cult Leader has no self-service way to do this from their own phone -- they tell you who to convert, and you act on it here. The host's own view never shows factions/characters (see the security note on view_for), so use the player IDs you assigned during setup, not names shown here. This is irreversible and secret -- double check before confirming." }
            PlayerSelect {
                roster: roster.clone(),
                placeholder: "-- converter (Cult Leader) --",
                on_change: move |e: FormEvent| {
                    convert_converter.set(e.value().parse().ok());
                    convert_armed.set(false);
                },
            }
            PlayerSelect {
                roster: roster.clone(),
                placeholder: "-- target --",
                on_change: move |e: FormEvent| {
                    convert_target.set(e.value().parse().ok());
                    convert_armed.set(false);
                },
            }
            button {
                disabled: convert_converter().is_none() || convert_target().is_none(),
                onclick: move |_| {
                    let (Some(converter), Some(target)) = (convert_converter(), convert_target())
                    else {
                        return;
                    };
                    if !convert_armed() {
                        convert_armed.set(true);
                        return;
                    }
                    do_cmd(Command::Convert {
                        converter: PlayerId(converter),
                        target: PlayerId(target),
                    });
                    convert_armed.set(false);
                    convert_converter.set(None);
                    convert_target.set(None);
                },
                if convert_armed() { "Confirm convert -- click again" } else { "Convert" }
            }
        }
        div {
            h3 { "Contest rounds (Round 2 & 4)" }
            p { "The actual mini-games are designed later -- this just records each category's result. Players never see the running standings or the breakdown (only the engine tracks it, for the Leader's Confidants)." }
            select {
                onchange: move |e| {
                    contest_round.set(if e.value() == "Four" { Round::Four } else { Round::Two });
                },
                option { value: "Two", "Round 2" }
                option { value: "Four", "Round 4" }
            }
            select {
                onchange: move |e| {
                    contest_category.set(match e.value().as_str() {
                        "Creativity" => ContestCategory::Creativity,
                        "Intelligence" => ContestCategory::Intelligence,
                        _ => ContestCategory::Strength,
                    });
                },
                option { value: "Strength", "Strength" }
                option { value: "Creativity", "Creativity" }
                option { value: "Intelligence", "Intelligence" }
            }
            select {
                onchange: move |e| contest_ton_won.set(e.value() == "Ton"),
                option { value: "Ton", "Ton won" }
                option { value: "Uprising", "Uprising won" }
            }
            button {
                onclick: move |_| {
                    do_cmd(Command::RecordContestResult {
                        round: contest_round(),
                        category: contest_category(),
                        ton_won: contest_ton_won(),
                    });
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
            p { "Draws up to 5 entrants from whoever opted in and is still active -- you don't get a names-and-opt-ins list (same no-ambient-god-view rule as everywhere else), so this runs the draw server-side instead of asking you to pick." }
            button {
                onclick: move |_| draw_intermission_entrants(),
                "Draw entrants"
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
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        let _ = socket.send(ClientMsg::Watch(Viewer::Display)).await;
        loop {
            match socket.recv().await {
                Ok(ServerMsg::View(v)) => view.set(Some(v)),
                Ok(
                    ServerMsg::Failed { .. }
                    | ServerMsg::Joined { .. }
                    | ServerMsg::LocationTaskTemplates(_),
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
            RosterList { roster: v.roster.clone() }
            match &v.denouncement {
                Some(DenouncementView::Nomination { .. }) => rsx! { p { "Nomination is open." } },
                Some(DenouncementView::Discussion { surfaced }) => rsx! {
                    p { "Up for the Denouncement: {names(surfaced, &v.roster)}" }
                },
                Some(DenouncementView::Ballot { candidates, .. } | DenouncementView::Runoff { candidates, .. }) => rsx! {
                    p { "Ballot open for: {names(candidates, &v.roster)}" }
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
