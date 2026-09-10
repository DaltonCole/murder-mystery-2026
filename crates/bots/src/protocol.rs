//! Mirrors `app`'s websocket wire format exactly (`ClientMsg`/`ServerMsg`
//! in `crates/app/src/main.rs`). Duplicated rather than shared because
//! `app` is a bin-only crate with no lib target -- same reasoning
//! game-changer's own `tests/api_integration.rs` and
//! `examples/simulate_party.rs` give for duplicating their own constants
//! rather than depending on their server crate as a library. Unlike
//! game-changer's raw-`serde_json::Value` approach, everything *except*
//! this thin wrapper enum comes straight from `engine`'s own
//! already-Serialize/Deserialize domain types, so there's far less to
//! keep in sync here -- if `app`'s protocol ever changes, drift here
//! shows up immediately as a JSON deserialization failure at connect
//! time, not a silent behavioral mismatch.

use engine::{Command, PlayerId, PlayerView, Viewer};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    Join { name: String },
    Watch(Viewer),
    Do(Command),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    Joined { player: PlayerId },
    View(PlayerView),
    Failed { error: String },
}

#[derive(Debug)]
pub enum ConnError {
    Ws(tokio_tungstenite::tungstenite::Error),
    Json(serde_json::Error),
    /// The socket closed before a message the caller was waiting for ever
    /// arrived.
    ClosedEarly,
    /// The server rejected a command/join with this error message.
    Rejected(String),
    /// `do_cmd_until` gave up waiting for its expected effect to show up
    /// in a pushed `View` before its deadline.
    Timeout(&'static str),
}

impl std::fmt::Display for ConnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnError::Ws(e) => write!(f, "websocket error: {e}"),
            ConnError::Json(e) => write!(f, "JSON error: {e}"),
            ConnError::ClosedEarly => write!(f, "connection closed before an expected reply"),
            ConnError::Rejected(e) => write!(f, "server rejected the request: {e}"),
            ConnError::Timeout(what) => write!(f, "timed out waiting for: {what}"),
        }
    }
}

impl std::error::Error for ConnError {}

impl From<tokio_tungstenite::tungstenite::Error> for ConnError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        ConnError::Ws(e)
    }
}

impl From<serde_json::Error> for ConnError {
    fn from(e: serde_json::Error) -> Self {
        ConnError::Json(e)
    }
}

/// One websocket connection to a real running `app` server, speaking the
/// exact JSON-over-text-frame protocol `dioxus::fullstack`'s typed
/// websocket uses (confirmed against a live server during this project's
/// own Phase 1 verification, not assumed).
pub struct Conn {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Conn {
    pub async fn connect(url: &str) -> Result<Self, ConnError> {
        let (socket, _response) = tokio_tungstenite::connect_async(url).await?;
        Ok(Conn { socket })
    }

    pub async fn send(&mut self, msg: &ClientMsg) -> Result<(), ConnError> {
        let text = serde_json::to_string(msg)?;
        self.socket.send(Message::Text(text.into())).await?;
        Ok(())
    }

    /// Reads the next `ServerMsg`, skipping any frame that isn't one
    /// (pings, etc. -- `tokio-tungstenite` answers pings automatically,
    /// this just needs to not choke if one arrives interleaved). Returns
    /// `Ok(None)` on a clean close.
    ///
    /// `dioxus::fullstack`'s typed websocket actually sends JSON as
    /// *binary* frames, not text -- confirmed by tracing a real
    /// connection during this crate's own development, not assumed from
    /// the spec (a text-only Python client used earlier in this project
    /// happened to still work purely because `websockets` decodes any
    /// UTF-8-valid frame the same way regardless of its declared type).
    /// Handle both so this doesn't depend on that being stable.
    pub async fn recv(&mut self) -> Result<Option<ServerMsg>, ConnError> {
        loop {
            match self.socket.next().await {
                Some(Ok(Message::Text(text))) => return Ok(Some(serde_json::from_str(&text)?)),
                Some(Ok(Message::Binary(bytes))) => {
                    return Ok(Some(serde_json::from_slice(&bytes)?))
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(e.into()),
                None => return Ok(None),
            }
        }
    }

    /// Sends `cmd`, then keeps consuming server messages -- correctly
    /// tolerating interleaved broadcast pushes from *other* connections'
    /// concurrent activity -- until either a `Failed` reply arrives (always
    /// attributable to this connection's own most recent request; the
    /// server never broadcasts `Failed` to anyone) or a pushed `View`
    /// satisfies `done`.
    ///
    /// This replaces the tempting-but-unsound "the very next message must
    /// be my reply" assumption: this connection's own `Do`'s response can
    /// arrive *after* several already-queued broadcast pushes from other
    /// connections' actions, since the server's per-connection event loop
    /// (see `app::game_ws`) processes and forwards messages strictly in
    /// arrival order, and a broadcast that was already pending when this
    /// `Do` was sent gets flushed out first. With many bots acting
    /// concurrently (exactly this crate's whole reason to exist), that's
    /// not a rare edge case -- it's the common case. Waiting for the
    /// *effect* `done` describes, rather than trusting order, is what
    /// makes this correct regardless of how many unrelated pushes land in
    /// between.
    pub async fn do_cmd_until(
        &mut self,
        cmd: Command,
        mut done: impl FnMut(&PlayerView) -> bool,
        timeout: Duration,
        what: &'static str,
    ) -> Result<PlayerView, ConnError> {
        self.send(&ClientMsg::Do(cmd)).await?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(ConnError::Timeout(what));
            }
            let next = match tokio::time::timeout(remaining, self.recv()).await {
                Ok(result) => result?,
                Err(_) => return Err(ConnError::Timeout(what)),
            };
            match next {
                Some(ServerMsg::View(v)) => {
                    if done(&v) {
                        return Ok(v);
                    }
                }
                Some(ServerMsg::Failed { error }) => return Err(ConnError::Rejected(error)),
                Some(ServerMsg::Joined { .. }) => {}
                None => return Err(ConnError::ClosedEarly),
            }
        }
    }

    /// Sends `cmd` and returns as soon as *any* reply lands, without
    /// checking that it's actually the direct response -- only safe to
    /// use when nothing else could plausibly be concurrently mutating
    /// state (e.g. sequential setup calls before any bot has anything to
    /// react to yet). Prefer `do_cmd_until` once concurrent activity is
    /// possible.
    pub async fn do_cmd_sequential(&mut self, cmd: Command) -> Result<PlayerView, ConnError> {
        self.send(&ClientMsg::Do(cmd)).await?;
        loop {
            match self.recv().await? {
                Some(ServerMsg::View(v)) => return Ok(v),
                Some(ServerMsg::Failed { error }) => return Err(ConnError::Rejected(error)),
                Some(ServerMsg::Joined { .. }) => continue,
                None => return Err(ConnError::ClosedEarly),
            }
        }
    }

    pub async fn watch(&mut self, viewer: Viewer) -> Result<PlayerView, ConnError> {
        self.send(&ClientMsg::Watch(viewer)).await?;
        loop {
            match self.recv().await? {
                Some(ServerMsg::View(v)) => return Ok(v),
                Some(ServerMsg::Joined { .. }) | Some(ServerMsg::Failed { .. }) => continue,
                None => return Err(ConnError::ClosedEarly),
            }
        }
    }

    /// Joins as a brand new player (mirrors a real `/play` connection's
    /// very first action) and returns the id the server assigned. Safe
    /// from the same ambiguity `do_cmd_until` guards against: `Joined` is
    /// never broadcast either, so it's unambiguously this connection's own
    /// reply the moment it arrives.
    pub async fn join(&mut self, name: &str) -> Result<PlayerId, ConnError> {
        self.send(&ClientMsg::Join { name: name.into() }).await?;
        loop {
            match self.recv().await? {
                Some(ServerMsg::Joined { player }) => return Ok(player),
                Some(ServerMsg::Failed { error }) => return Err(ConnError::Rejected(error)),
                Some(ServerMsg::View(_)) => continue,
                None => return Err(ConnError::ClosedEarly),
            }
        }
    }
}
