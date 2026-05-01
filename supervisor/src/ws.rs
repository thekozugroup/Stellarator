use crate::api::{secrets_match, AUTH_QUERY};
use crate::supervisor::MetricUpdate;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Per-run channel capacity. We intentionally use a bounded broadcast channel
/// with drop-oldest semantics (tokio's broadcast lags slow subscribers rather
/// than blocking publishers). For a UI metrics stream this lossy fanout is
/// acceptable: the latest sample is what the user cares about, and any gap is
/// surfaced as a "lagged" warning rather than back-pressure on the trainer.
const RUN_CHAN_CAP: usize = 256;
/// Global fanout channel — same drop-oldest semantics, sized for many viewers.
const FANOUT_CAP: usize = 1024;

/// Hub holds a per-run broadcast channel + a global fanout channel.
/// Receivers automatically drop when the WebSocket closes.
pub struct Hub {
    per_run: DashMap<String, broadcast::Sender<MetricUpdate>>,
    fanout: broadcast::Sender<MetricUpdate>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        let (fanout_tx, _) = broadcast::channel(FANOUT_CAP);
        Self {
            per_run: DashMap::new(),
            fanout: fanout_tx,
        }
    }

    fn run_sender(&self, run_id: &str) -> broadcast::Sender<MetricUpdate> {
        self.per_run
            .entry(run_id.to_string())
            .or_insert_with(|| broadcast::channel(RUN_CHAN_CAP).0)
            .clone()
    }

    pub fn publish(&self, run_id: &str, update: MetricUpdate) {
        let _ = self.run_sender(run_id).send(update.clone());
        let _ = self.fanout.send(update);
    }

    pub fn subscribe_run(&self, run_id: &str) -> broadcast::Receiver<MetricUpdate> {
        self.run_sender(run_id).subscribe()
    }

    pub fn subscribe_all(&self) -> broadcast::Receiver<MetricUpdate> {
        self.fanout.subscribe()
    }
}

#[derive(Clone)]
pub struct WsState {
    pub hub: Arc<Hub>,
    pub shared_secret: Arc<String>,
    pub shutdown: CancellationToken,
}

#[derive(Debug, Deserialize)]
pub struct AuthQuery {
    #[serde(default)]
    pub token: Option<String>,
}

fn ws_authorized(state: &WsState, q: &HashMap<String, String>) -> bool {
    let provided = q
        .get(AUTH_QUERY)
        .map(String::as_str)
        .unwrap_or("");
    secrets_match(provided, &state.shared_secret)
}

pub async fn ws_run(
    State(state): State<WsState>,
    Path(run_id): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    if !crate::api::is_uuid_shaped(&run_id) {
        return (StatusCode::BAD_REQUEST, "invalid run_id").into_response();
    }
    if !ws_authorized(&state, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let rx = state.hub.subscribe_run(&run_id);
    let shutdown = state.shutdown.clone();
    ws.on_upgrade(move |socket| pump(socket, rx, Some(run_id), shutdown))
}

pub async fn ws_all(
    State(state): State<WsState>,
    Query(q): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Response {
    if !ws_authorized(&state, &q) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let rx = state.hub.subscribe_all();
    let shutdown = state.shutdown.clone();
    ws.on_upgrade(move |socket| pump(socket, rx, None, shutdown))
}

async fn pump(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<MetricUpdate>,
    scope: Option<String>,
    shutdown: CancellationToken,
) {
    debug!(?scope, "ws connected");
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                // RFC 6455 close code 1001 = "going away".
                let _ = socket.send(Message::Close(Some(CloseFrame {
                    code: 1001,
                    reason: Cow::Borrowed("server shutting down"),
                }))).await;
                break;
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => { warn!(error=?e, "ws recv"); break; }
                    _ => {}
                }
            }
            evt = rx.recv() => {
                match evt {
                    Ok(update) => {
                        let payload = match serde_json::to_string(&update) {
                            Ok(s) => s,
                            Err(e) => { warn!(error=?e, "ws encode"); continue; }
                        };
                        if socket.send(Message::Text(payload)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped=n, ?scope, "ws subscriber lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    debug!(?scope, "ws disconnected");
}
