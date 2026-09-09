//! Phase 0 entry point: the three role-based routes from the implementation
//! plan (`/`=play, `/host`, `/display`), plus a websocket round-trip
//! spiking the realtime mechanism the whole app depends on. This gets
//! replaced by real game-state broadcasting in later phases -- see
//! `/home/drc/.claude/plans/piped-crunching-lighthouse.md`.

use dioxus::fullstack::{use_websocket, Websocket, WebSocketOptions};
use dioxus::prelude::*;
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

#[component]
fn Play() -> Element {
    rsx! {
        h1 { "Murder Mystery 2026 -- Play" }
        WebsocketSpike {}
    }
}

#[component]
fn Host() -> Element {
    rsx! {
        h1 { "Murder Mystery 2026 -- Host Console" }
    }
}

#[component]
fn Display() -> Element {
    rsx! {
        h1 { "Murder Mystery 2026 -- Display" }
    }
}

// --- Phase 0 websocket spike -----------------------------------------
//
// Proves the realtime mechanism the "Networking, Realtime, and
// Authorization" section of the plan is built on: a typed websocket with
// the server able to push messages the client didn't ask for (not just a
// request/reply server function). Real usage sends `DomainEvent`s /
// `PlayerView`s here instead of a plain string echo.

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ClientMsg {
    Ping(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ServerMsg {
    Pong(String),
}

#[get("/api/ws")]
async fn game_ws(options: WebSocketOptions) -> Result<Websocket<ClientMsg, ServerMsg>> {
    Ok(options.on_upgrade(move |mut socket| async move {
        while let Ok(msg) = socket.recv().await {
            let ClientMsg::Ping(text) = msg;
            if socket
                .send(ServerMsg::Pong(format!("echo: {text}")))
                .await
                .is_err()
            {
                break;
            }
        }
    }))
}

#[component]
fn WebsocketSpike() -> Element {
    let mut log = use_signal(Vec::<String>::new);
    let mut draft = use_signal(String::new);
    let mut socket = use_websocket(|| game_ws(WebSocketOptions::new()));

    use_future(move || async move {
        while let Ok(ServerMsg::Pong(text)) = socket.recv().await {
            log.write().push(text);
        }
    });

    rsx! {
        div {
            id: "ws-spike",
            h4 { "Websocket spike" }
            input {
                placeholder: "Type and press Enter to ping the server...",
                value: "{draft}",
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event: Event<KeyboardData>| {
                    if event.key() == Key::Enter {
                        let value = draft.peek().clone();
                        spawn(async move {
                            let _ = socket.send(ClientMsg::Ping(value)).await;
                        });
                        draft.set(String::new());
                    }
                },
            }
            ul {
                for (i, line) in log.read().iter().enumerate() {
                    li { key: "{i}", "{line}" }
                }
            }
        }
    }
}
