//! WebRTC connection management
//!
//! Handles WebRTC peer connection and data channels

use anyhow::{Context, Result};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use webrtc::{
    api::{APIBuilder, API},
    peer_connection::{
        configuration::RTCConfiguration,
        peer_connection_state::PeerConnectionState,
        RTCPeerConnection,
    },
    data_channel::{RTCDataChannel, data_channel_message::DataChannelMessage},
    ice_server::RTCIceServer,
};

/// WebRTC connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebRtcState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

/// WebRTC connection wrapper
pub struct WebRtcConnection {
    /// WebRTC API
    api: API,
    /// Peer connection
    pc: Arc<RTCPeerConnection>,
    /// Data channel (lazy initialized)
    data_channel: Arc<Mutex<Option<Arc<RTCDataChannel>>>>,
    /// Message receiver
    rx: Mutex<mpsc::Receiver<Bytes>>,
    /// Connection state
    state: Arc<Mutex<WebRtcState>>,
}

impl WebRtcConnection {
    /// Create a new WebRTC connection (as offerer/host)
    pub async fn new_host(stun_servers: Vec<String>) -> Result<Self> {
        // Build WebRTC API
        let api = APIBuilder::new().build();

        // Configure peer connection
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: stun_servers
                    .into_iter()
                    .map(|s| format!("stun:{}", s))
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        };

        // Create peer connection
        let pc = Arc::new(
            api.new_peer_connection(config)
                .await
                .context("Failed to create peer connection")?,
        );

        // Create message channel
        let (tx, rx) = mpsc::channel::<Bytes>(100);
        let rx = Mutex::new(rx);

        // Clone for state callback
        let state = Arc::new(Mutex::new(WebRtcState::Disconnected));
        let state_clone = Arc::clone(&state);
        let pc_clone = Arc::clone(&pc);

        // Set up state change handler
        let mut on_connection_state_change = Box::new(move |state: PeerConnectionState| {
            let state_clone = Arc::clone(&state_clone);
            Box::pin(async move {
                let mut current_state = state_clone.lock().await;
                *current_state = match state {
                    PeerConnectionState::New => WebRtcState::Connecting,
                    PeerConnectionState::Connecting => WebRtcState::Connecting,
                    PeerConnectionState::Connected => WebRtcState::Connected,
                    PeerConnectionState::Disconnected => WebRtcState::Disconnected,
                    PeerConnectionState::Failed => WebRtcState::Failed,
                    PeerConnectionState::Closed => WebRtcState::Disconnected,
                };
                tracing::info!("WebRTC connection state changed to {:?}", *current_state);
            })
        });
        pc_clone.on_peer_connection_state_change(on_connection_state_change);

        // Data channel will be created when peer connects
        let data_channel = Arc::new(Mutex::new(None));
        let data_channel_clone = Arc::clone(&data_channel);

        // Set up data channel handler
        let on_data_channel = Box::new(move |d: Arc<RTCDataChannel>| {
            let data_channel_clone = Arc::clone(&data_channel_clone);
            Box::pin(async move {
                tracing::info!("Data channel established: {}", d.label());

                // Set up message handler
                let mut on_message_handler = Box::new(move |msg: DataChannelMessage| {
                    let tx_clone = tx.clone();
                    Box::pin(async move {
                        if !msg.is_string {
                            let _ = tx_clone.send(Bytes::from(msg.data)).await;
                        }
                    })
                });
                d.on_message(on_message_handler);

                // Store the data channel
                let mut dc = data_channel_clone.lock().await;
                *dc = Some(d);
            })
        });
        pc_clone.on_data_channel(on_data_channel);

        Ok(Self {
            api,
            pc,
            data_channel,
            rx,
            state,
        })
    }

    /// Create an SDP offer
    pub async fn create_offer(&self) -> Result<String> {
        // Create data channel first (required for the connection)
        let _ = self
            .pc
            .create_data_channel("claude-code", None)
            .await
            .context("Failed to create data channel")?;

        tracing::info!("Created offer data channel");

        // Create offer
        let offer = self
            .pc
            .create_offer(None)
            .await
            .context("Failed to create offer")?;

        // Set local description
        self.pc
            .set_local_description(offer)
            .await
            .context("Failed to set local description")?;

        // Wait for ICE gathering to complete
        // In production, you'd use trickle ICE
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Return SDP
        Ok(self.pc.local_description().await.sdp)
    }

    /// Set remote SDP answer
    pub async fn set_answer(&self, sdp: String) -> Result<()> {
        let answer = webrtc::peer_connection::session_description::RTCSessionDescription {
            sdp_type: webrtc::peer_connection::sdp_type::RTCSdpType::Answer,
            sdp,
        };

        self.pc
            .set_remote_description(answer)
            .await
            .context("Failed to set remote description")?;

        tracing::info!("Set remote answer");
        Ok(())
    }

    /// Wait for connection to be established
    pub async fn wait_connected(&self, timeout_secs: u64) -> Result<()> {
        let timeout = tokio::time::Duration::from_secs(timeout_secs);
        tokio::time::timeout(timeout, async {
            loop {
                {
                    let state = self.state.lock().await;
                    if *state == WebRtcState::Connected {
                        return Ok(());
                    }
                    if *state == WebRtcState::Failed {
                        return Err(anyhow::anyhow!("Connection failed"));
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .context("Connection timeout")?
    }

    /// Send data over the data channel
    pub async fn send(&self, data: Bytes) -> Result<()> {
        let dc = self.data_channel.lock().await;
        if let Some(channel) = dc.as_ref() {
            channel
                .send(&data)
                .await
                .context("Failed to send data")?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Data channel not initialized"))
        }
    }

    /// Receive data from the data channel
    pub async fn recv(&self) -> Option<Bytes> {
        let mut rx = self.rx.lock().await;
        rx.recv().await
    }

    /// Get current connection state
    pub async fn state(&self) -> WebRtcState {
        *self.state.lock().await
    }

    /// Close the connection
    pub async fn close(&self) -> Result<()> {
        self.pc.close().await.context("Failed to close connection")
    }
}
