//! P2P Claude Code Signaling Server
//!
//! Minimal signaling server for WebRTC SDP exchange.
//! Only handles pairing and offer/answer exchange, no media relay.

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    routing::get,
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

/// Signaling message types
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalingMessage {
    /// Client initiates pairing, server responds with pairing code
    Init { client_id: String },
    /// Client provides pairing code to connect
    Pair { pairing_code: String },
    /// WebRTC offer
    Offer { sdp: String },
    /// WebRTC answer
    Answer { sdp: String },
    /// Pairing successful
    Paired { peer_id: String },
    /// Error message
    Error { message: String },
    /// Success acknowledgment
    Ok,
}

/// Connection state for a client
struct ClientState {
    ws_tx: broadcast::Sender<SignalingMessage>,
    pairing_code: Option<String>,
    paired_with: Option<String>,
}

/// Shared server state
#[derive(Clone)]
struct AppState {
    clients: Arc<DashMap<String, ClientState>>,
    pairing_codes: Arc<DashMap<String, String>>, // code -> client_id
}

impl AppState {
    fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            pairing_codes: Arc::new(DashMap::new()),
        }
    }

    /// Generate a unique pairing code
    fn generate_pairing_code(&self) -> String {
        // Generate 6-digit code
        format!("{:06}", rand::random::<u32>() % 1_000_000)
    }

    /// Register a new client and return pairing code
    async fn register_client(&self, client_id: String) -> String {
        let (tx, _) = broadcast::channel::<SignalingMessage>(100);
        let pairing_code = self.generate_pairing_code();

        self.pairing_codes
            .insert(pairing_code.clone(), client_id.clone());

        self.clients.insert(
            client_id,
            ClientState {
                ws_tx: tx,
                pairing_code: Some(pairing_code.clone()),
                paired_with: None,
            },
        );

        tracing::info!("Client registered: {} with code {}", client_id, pairing_code);
        pairing_code
    }

    /// Pair two clients together
    async fn pair_clients(&self, code: String, client_id: String) -> Option<String> {
        // Find the host client by pairing code
        if let Some((_, host_id)) = self.pairing_codes.remove(&code) {
            // Update both clients' state
            if let Some(mut host) = self.clients.get_mut(&host_id) {
                host.paired_with = Some(client_id.clone());
            }

            if let Some(mut client) = self.clients.get_mut(&client_id) {
                client.paired_with = Some(host_id.clone());
            }

            tracing::info!("Paired {} with {}", client_id, host_id);
            return Some(host_id);
        }
        None
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let state = AppState::new();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state);

    let addr = "0.0.0.0:8080";
    tracing::info!("Signaling server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Generate client ID
    let client_id = Uuid::new_v4().to_string();

    // Register client
    let pairing_code = state.register_client(client_id.clone()).await;

    // Send pairing code to client
    let init_msg = SignalingMessage::Init {
        client_id: client_id.clone(),
    };
    if let Ok(json) = serde_json::to_string(&init_msg) {
        let _ = sender.send(Message::Text(json.into())).await;
    }

    tracing::info!("WebSocket client connected: {}", client_id);

    // Handle incoming messages
    while let Some(msg) = receiver.recv().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(signal_msg) = serde_json::from_str::<SignalingMessage>(&text) {
                    match signal_msg {
                        SignalingMessage::Pair { pairing_code } => {
                            match state.pair_clients(pairing_code, client_id.clone()).await {
                                Some(peer_id) => {
                                    let response = SignalingMessage::Paired { peer_id };
                                    let json = serde_json::to_string(&response).unwrap();
                                    let _ = sender.send(Message::Text(json.into())).await;
                                }
                                None => {
                                    let response = SignalingMessage::Error {
                                        message: "Invalid pairing code".into(),
                                    };
                                    let json = serde_json::to_string(&response).unwrap();
                                    let _ = sender.send(Message::Text(json.into())).await;
                                }
                            }
                        }
                        SignalingMessage::Offer { sdp } => {
                            // Forward offer to paired peer
                            if let Some(client) = state.clients.get(&client_id) {
                                if let Some(peer_id) = &client.paired_with {
                                    if let Some(peer) = state.clients.get(peer_id) {
                                        let _ = peer.ws_tx.send(SignalingMessage::Offer { sdp });
                                    }
                                }
                            }
                        }
                        SignalingMessage::Answer { sdp } => {
                            // Forward answer to paired peer
                            if let Some(client) = state.clients.get(&client_id) {
                                if let Some(peer_id) = &client.paired_with {
                                    if let Some(peer) = state.clients.get(peer_id) {
                                        let _ = peer.ws_tx.send(SignalingMessage::Answer { sdp });
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("WebSocket client disconnected: {}", client_id);
                state.clients.remove(&client_id);
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}
