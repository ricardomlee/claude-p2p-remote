//! Signaling client for SDP exchange
//!
//! WebSocket client for exchanging WebRTC SDP offers/answers

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{ClientRequestBuilder, Message},
    MaybeTlsStream, WebSocketStream,
};
use tracing;

/// Signaling message for WebRTC SDP exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalingMessage {
    /// Initial connection message
    Init { client_id: String },
    /// Pair with host using code
    Pair { pairing_code: String },
    /// WebRTC offer with SDP
    Offer { sdp: String },
    /// WebRTC answer with SDP
    Answer { sdp: String },
    /// Pairing successful
    Paired { peer_id: String },
    /// Error message
    Error { message: String },
    /// Acknowledgment
    Ok,
}

/// Signaling client for WebRTC handshake
pub struct SignalingClient {
    /// WebSocket stream
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// Our client ID (assigned after init)
    client_id: Option<String>,
}

impl SignalingClient {
    /// Connect to signaling server
    pub async fn connect(url: &str) -> Result<Self> {
        let ws_url = url.replace("http://", "ws://").replace("https://", "wss://");

        let (ws, response) = connect_async(ws_url)
            .await
            .context("Failed to connect to signaling server")?;

        tracing::info!("Connected to signaling server: {:?}", response.status());

        Ok(Self {
            ws,
            client_id: None,
        })
    }

    /// Send initial message and get client ID
    pub async fn init(&mut self) -> Result<String> {
        // Send init message
        let init_msg = SignalingMessage::Init {
            client_id: "auto".to_string(), // Server will assign actual ID
        };

        self.send_message(init_msg).await?;

        // Wait for response with our ID
        let msg = self.recv_message().await?;

        match msg {
            SignalingMessage::Init { client_id } => {
                self.client_id = Some(client_id.clone());
                Ok(client_id)
            }
            SignalingMessage::Error { message } => {
                Err(anyhow::anyhow!("Init failed: {}", message))
            }
            _ => Err(anyhow::anyhow!("Unexpected response to init")),
        }
    }

    /// Pair with host using pairing code
    pub async fn pair(&mut self, code: &str) -> Result<String> {
        self.send_message(SignalingMessage::Pair {
            pairing_code: code.to_string(),
        })
        .await?;

        let msg = self.recv_message().await?;

        match msg {
            SignalingMessage::Paired { peer_id } => Ok(peer_id),
            SignalingMessage::Error { message } => {
                Err(anyhow::anyhow!("Pairing failed: {}", message))
            }
            _ => Err(anyhow::anyhow!("Unexpected response to pair")),
        }
    }

    /// Send WebRTC offer
    pub async fn send_offer(&mut self, sdp: String) -> Result<()> {
        self.send_message(SignalingMessage::Offer { sdp }).await
    }

    /// Send WebRTC answer
    pub async fn send_answer(&mut self, sdp: String) -> Result<()> {
        self.send_message(SignalingMessage::Answer { sdp }).await
    }

    /// Wait for and receive an offer
    pub async fn recv_offer(&mut self) -> Result<String> {
        loop {
            let msg = self.recv_message().await?;
            if let SignalingMessage::Offer { sdp } = msg {
                return Ok(sdp);
            }
        }
    }

    /// Wait for and receive an answer
    pub async fn recv_answer(&mut self) -> Result<String> {
        loop {
            let msg = self.recv_message().await?;
            if let SignalingMessage::Answer { sdp } = msg {
                return Ok(sdp);
            }
        }
    }

    /// Send a signaling message
    async fn send_message(&mut self, msg: SignalingMessage) -> Result<()> {
        let json = serde_json::to_string(&msg).context("Failed to serialize message")?;
        self.ws
            .send(Message::Text(json.into()))
            .await
            .context("Failed to send message")?;
        Ok(())
    }

    /// Receive a signaling message
    async fn recv_message(&mut self) -> Result<SignalingMessage> {
        loop {
            let msg = self
                .ws
                .next()
                .await
                .context("Connection closed")?
                .context("Failed to receive message")?;

            match msg {
                Message::Text(text) => {
                    let signal_msg: SignalingMessage =
                        serde_json::from_str(&text).context("Failed to parse message")?;
                    return Ok(signal_msg);
                }
                Message::Ping(data) => {
                    self.ws.send(Message::Pong(data)).await?;
                }
                Message::Close(_) => {
                    return Err(anyhow::anyhow!("Connection closed by server"));
                }
                _ => continue,
            }
        }
    }

    /// Close the connection
    pub async fn close(&mut self) -> Result<()> {
        self.ws
            .close(None)
            .await
            .context("Failed to close connection")
    }
}
