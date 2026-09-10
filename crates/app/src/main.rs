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
//!   Player's.
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
#[cfg(feature = "server")]
use engine::DomainEvent;
use engine::{
    AbilityStatus, Ballot, Character, Command, ContestCategory, DenouncementView, Faction,
    GalleryPrediction, InfoCheckAnswer, InfoCheckDelivery, InfoQueryKind, PlayerId, PlayerStatus,
    PlayerView, RosterEntry, Round, TaskTier, TaskView, Viewer,
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

#[component]
fn App() -> Element {
    rsx! {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ServerMsg {
    Joined { player: PlayerId },
    View(PlayerView),
    Failed { error: String },
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
                            socket.send(ServerMsg::View(game_server::view(v))).await.is_ok()
                        }
                        ClientMsg::Do(cmd) => match game_server::apply(cmd) {
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
                        },
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
                    button {
                        disabled: !abilities.potion_maker_available.unwrap_or(false),
                        onclick: move |_| on_command.call(Command::ActivatePotionImmunity { player: my_id }),
                        "Activate execution immunity",
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

// --- /host -----------------------------------------------------------------

#[component]
fn Host() -> Element {
    let mut view = use_signal(|| None::<PlayerView>);
    let mut error = use_signal(|| None::<String>);
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
    let mut contest_round = use_signal(|| Round::Two);
    let mut contest_category = use_signal(|| ContestCategory::Strength);
    let mut contest_ton_won = use_signal(|| true);
    let mut intermission_selected = use_signal(String::new);
    let mut servant_award_player = use_signal(|| None::<u32>);
    let mut servant_award_points = use_signal(|| 1u32);
    let mut gallery_cast_out = use_signal(String::new);
    let mut gallery_winner = use_signal(|| Faction::Ton);

    let roster = view().map(|v| v.roster).unwrap_or_default();

    rsx! {
        h1 { "Host Console" }
        if let Some(e) = error() {
            p { style: "color:red", "{e}" }
        }
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
            div {
                select {
                    onchange: move |e| faction_player.set(e.value().parse().ok()),
                    option { value: "", "-- player --" }
                    for r in roster.clone() {
                        option { value: "{r.id.0}", "{r.name}" }
                    }
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
                select {
                    onchange: move |e| character_player.set(e.value().parse().ok()),
                    option { value: "", "-- player --" }
                    for r in roster.clone() {
                        option { value: "{r.id.0}", "{r.name}" }
                    }
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
            p { "Assign a faction to every player first, assign the four titles above to their holders, then Finalize -- everyone else gets a generic character automatically." }
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
            select {
                onchange: move |e| task_qualifier.set(e.value().parse().ok()),
                option { value: "", "-- who qualifies? --" }
                for r in roster.clone() {
                    option { value: "{r.id.0}", "{r.name}" }
                }
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
                    });
                    task_prompt.set(String::new());
                },
                "Push task"
            }
            button { onclick: move |_| do_cmd(Command::CloseTasks), "Close tasks" }
        }
        div {
            h3 { "Debug: Cult conversion" }
            p { "The real recruitment schedule is Phase 2 -- this is a manual stand-in. The host's own view never shows factions/characters (see the security note on view_for), so use the player IDs you assigned above, not names shown here." }
            select {
                onchange: move |e| convert_converter.set(e.value().parse().ok()),
                option { value: "", "-- converter (Cult Leader) --" }
                for r in roster.clone() {
                    option { value: "{r.id.0}", "{r.name}" }
                }
            }
            select {
                onchange: move |e| convert_target.set(e.value().parse().ok()),
                option { value: "", "-- target --" }
                for r in roster.clone() {
                    option { value: "{r.id.0}", "{r.name}" }
                }
            }
            button {
                disabled: convert_converter().is_none() || convert_target().is_none(),
                onclick: move |_| {
                    let (Some(converter), Some(target)) = (convert_converter(), convert_target())
                    else {
                        return;
                    };
                    do_cmd(Command::Convert {
                        converter: PlayerId(converter),
                        target: PlayerId(target),
                    });
                },
                "Convert"
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
        }
        div {
            h3 { "Intermission lottery" }
            p { "Draw the 5 entrants from whoever opted in (comma-separated player IDs -- the host doesn't get a names-and-opt-ins list here, per the same no-ambient-god-view rule as everything else)." }
            input {
                placeholder: "e.g. 2,5,9",
                value: "{intermission_selected}",
                oninput: move |e| intermission_selected.set(e.value()),
            }
            button {
                onclick: move |_| {
                    let selected: Vec<PlayerId> = intermission_selected
                        .peek()
                        .split(',')
                        .filter_map(|s| s.trim().parse::<u32>().ok())
                        .map(PlayerId)
                        .collect();
                    do_cmd(Command::DrawIntermissionEntrants { selected });
                    intermission_selected.set(String::new());
                },
                "Draw entrants"
            }
        }
        div {
            h3 { "Servant leaderboard" }
            p { "Any Servant, or any already-Cast-Out player, is eligible. What earns points (zone scorekeeping, trivia, a minigame) is up to you -- the app just tracks the running total." }
            select {
                onchange: move |e| servant_award_player.set(e.value().parse().ok()),
                option { value: "", "-- player --" }
                for r in roster.clone() {
                    option { value: "{r.id.0}", "{r.name}" }
                }
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
            p { "Once at the Finale: score every submitted Gallery prediction against the real outcome." }
            input {
                placeholder: "actual Cast-Out IDs, e.g. 2,5",
                value: "{gallery_cast_out}",
                oninput: move |e| gallery_cast_out.set(e.value()),
            }
            select {
                onchange: move |e| {
                    gallery_winner.set(match e.value().as_str() {
                        "Uprising" => Faction::Uprising,
                        "Cult" => Faction::Cult,
                        _ => Faction::Ton,
                    });
                },
                option { value: "Ton", "Ton wins" }
                option { value: "Uprising", "Uprising wins" }
                option { value: "Cult", "Cult wins" }
            }
            button {
                onclick: move |_| {
                    let actual_cast_out: Vec<PlayerId> = gallery_cast_out
                        .peek()
                        .split(',')
                        .filter_map(|s| s.trim().parse::<u32>().ok())
                        .map(PlayerId)
                        .collect();
                    do_cmd(Command::ResolveGalleryPredictions {
                        actual_cast_out,
                        actual_winner: gallery_winner(),
                    });
                },
                "Resolve Gallery"
            }
        }
        RosterList { roster }
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
                Ok(ServerMsg::Failed { .. } | ServerMsg::Joined { .. }) => {}
                // See the identical comment in `Host` -- without this, a
                // closed connection spins this loop forever with no yield.
                Err(_) => break,
            }
        }
    });

    let Some(v) = view() else {
        return rsx! { h1 { "Murder Mystery 2026" } };
    };

    rsx! {
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
    }
}
