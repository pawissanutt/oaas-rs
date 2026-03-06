//! WebSocket event subscription handler.
//!
//! When the Gateway has `GATEWAY_WS_ENABLED=true`, these routes allow clients
//! to receive real-time ODGM state-change events via WebSocket.
//!
//! Events are received from Zenoh (published by ODGM's V2 dispatcher) and
//! forwarded as JSON frames to connected WS clients.

use axum::{
    Extension,
    extract::{Path, WebSocketUpgrade, ws::Message},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, info, warn};

/// Subscribe to events for a single object.
/// Route: `GET /api/class/{cls}/{pid}/objects/{oid}/ws`
pub async fn ws_object_handler(
    Path((cls, pid, oid)): Path<(String, u16, String)>,
    ws: WebSocketUpgrade,
    Extension(z_session): Extension<zenoh::Session>,
) -> impl IntoResponse {
    let topic = format!("oprc/{}/{}/events/{}", cls, pid, oid);
    info!(topic = %topic, "WS upgrade: object subscription");
    ws.on_upgrade(move |socket| handle_ws(socket, z_session, topic))
}

/// Subscribe to events for all objects in a partition.
/// Route: `GET /api/class/{cls}/{pid}/ws`
pub async fn ws_partition_handler(
    Path((cls, pid)): Path<(String, u16)>,
    ws: WebSocketUpgrade,
    Extension(z_session): Extension<zenoh::Session>,
) -> impl IntoResponse {
    let topic = format!("oprc/{}/{}/events/**", cls, pid);
    info!(topic = %topic, "WS upgrade: partition subscription");
    ws.on_upgrade(move |socket| handle_ws(socket, z_session, topic))
}

/// Subscribe to events for all objects across all partitions in a class.
/// Route: `GET /api/class/{cls}/ws`
pub async fn ws_class_handler(
    Path(cls): Path<String>,
    ws: WebSocketUpgrade,
    Extension(z_session): Extension<zenoh::Session>,
) -> impl IntoResponse {
    let topic = format!("oprc/{}/*/events/**", cls);
    info!(topic = %topic, "WS upgrade: class subscription");
    ws.on_upgrade(move |socket| handle_ws(socket, z_session, topic))
}

/// Core WS loop: subscribe to Zenoh topic, forward events as JSON text frames.
/// Exits on client disconnect or Zenoh subscription failure.
async fn handle_ws(
    socket: axum::extract::ws::WebSocket,
    z_session: zenoh::Session,
    topic: String,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Declare Zenoh subscriber for the event topic
    let subscriber = match z_session.declare_subscriber(&topic).await {
        Ok(sub) => sub,
        Err(e) => {
            warn!(topic = %topic, error = %e, "Failed to declare Zenoh subscriber for WS");
            let _ = ws_tx
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011,
                    reason: format!("Zenoh subscribe failed: {}", e).into(),
                })))
                .await;
            return;
        }
    };

    debug!(topic = %topic, "Zenoh subscriber active, forwarding to WS");

    loop {
        tokio::select! {
            // Forward Zenoh events → WS
            sample = subscriber.recv_async() => {
                match sample {
                    Ok(sample) => {
                        let payload = sample.payload().to_bytes();
                        if ws_tx.send(Message::Text(
                            String::from_utf8_lossy(&payload).into_owned().into()
                        )).await.is_err() {
                            debug!(topic = %topic, "WS send failed (client disconnected)");
                            break;
                        }
                    }
                    Err(_) => {
                        debug!(topic = %topic, "Zenoh subscriber closed");
                        break;
                    }
                }
            }
            // Watch for client disconnect / control frames
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        debug!(topic = %topic, "WS client disconnected");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_tx.send(Message::Pong(data)).await;
                    }
                    _ => {} // ignore text/binary from client
                }
            }
        }
    }

    info!(topic = %topic, "WS session ended, cleaning up Zenoh subscriber");
    // subscriber is dropped here, which undeclares it
}
