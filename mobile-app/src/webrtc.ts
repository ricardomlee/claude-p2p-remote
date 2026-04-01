// WebRTC connection for mobile app

export type WebRTCState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'failed';

export class WebRTCConnection {
  private peerConnection: RTCPeerConnection | null = null;
  private dataChannel: RTCDataChannel | null = null;
  private signalingUrl: string;
  private ws: WebSocket | null = null;

  public onMessage: ((data: ArrayBuffer) => void) | null = null;
  public onStateChange: ((state: WebRTCState) => void) | null = null;

  constructor(signalingUrl: string) {
    this.signalingUrl = signalingUrl;
  }

  /**
   * Connect to host using pairing code
   */
  async connect(pairingCode: string): Promise<void> {
    this.updateState('connecting');

    // Create peer connection
    this.peerConnection = new RTCPeerConnection({
      iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
    });

    // Set up ICE connection state handler
    this.peerConnection.oniceconnectionstatechange = () => {
      console.log('ICE state:', this.peerConnection?.iceConnectionState);
      switch (this.peerConnection?.iceConnectionState) {
        case 'connected':
          this.updateState('connected');
          break;
        case 'failed':
          this.updateState('failed');
          break;
        case 'disconnected':
          this.updateState('disconnected');
          break;
      }
    };

    // Set up data channel handler
    this.peerConnection.ondatachannel = (event) => {
      console.log('Data channel received:', event.channel.label);
      this.setupDataChannel(event.channel);
    };

    // Connect to signaling server
    await this.connectToSignaling();

    // Send pair message
    this.sendSignalingMessage({
      type: 'pair',
      pairing_code: pairingCode,
    });

    // Wait for paired response
    const peerId = await this.waitForPaired();
    console.log('Paired with:', peerId);

    // Create and send offer
    const offer = await this.peerConnection.createOffer();
    await this.peerConnection.setLocalDescription(offer);

    // Wait for ICE gathering
    await this.waitForIceGathering();

    // Send offer
    this.sendSignalingMessage({
      type: 'offer',
      sdp: this.peerConnection.localDescription?.sdp || '',
    });

    // Wait for answer
    const answerSdp = await this.waitForAnswer();

    // Set remote description
    await this.peerConnection.setRemoteDescription({
      type: 'answer',
      sdp: answerSdp,
    });
  }

  /**
   * Send data over the data channel
   */
  send(data: Uint8Array): void {
    if (this.dataChannel && this.dataChannel.readyState === 'open') {
      this.dataChannel.send(data);
    } else {
      console.warn('Data channel not ready');
    }
  }

  /**
   * Close the connection
   */
  close(): void {
    if (this.dataChannel) {
      this.dataChannel.close();
    }
    if (this.peerConnection) {
      this.peerConnection.close();
    }
    if (this.ws) {
      this.ws.close();
    }
  }

  /**
   * Set up data channel handlers
   */
  private setupDataChannel(channel: RTCDataChannel): void {
    this.dataChannel = channel;

    channel.onopen = () => {
      console.log('Data channel opened');
      this.updateState('connected');
    };

    channel.onmessage = (event) => {
      if (this.onMessage) {
        if (typeof event.data === 'string') {
          // Convert string to ArrayBuffer
          const encoder = new TextEncoder();
          this.onMessage(encoder.encode(event.data).buffer);
        } else {
          this.onMessage(event.data);
        }
      }
    };

    channel.onclose = () => {
      console.log('Data channel closed');
      this.updateState('disconnected');
    };
  }

  /**
   * Connect to signaling server
   */
  private async connectToSignaling(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.signalingUrl);

      this.ws.onopen = () => {
        console.log('Connected to signaling server');
        resolve();
      };

      this.ws.onerror = (e) => {
        console.error('Signaling connection error:', e);
        reject('Failed to connect to signaling server');
      };
    });
  }

  /**
   * Send message to signaling server
   */
  private sendSignalingMessage(msg: object): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  /**
   * Wait for paired response
   */
  private async waitForPaired(): Promise<string> {
    return new Promise((resolve, reject) => {
      if (!this.ws) {
        reject('No signaling connection');
        return;
      }

      const handler = (event: MessageEvent) => {
        const msg = JSON.parse(event.data);
        if (msg.type === 'paired') {
          this.ws!.removeEventListener('message', handler);
          resolve(msg.peer_id);
        } else if (msg.type === 'error') {
          this.ws!.removeEventListener('message', handler);
          reject(msg.message);
        }
      };

      this.ws.addEventListener('message', handler);

      // Timeout after 30 seconds
      setTimeout(() => {
        reject('Pairing timeout');
      }, 30000);
    });
  }

  /**
   * Wait for SDP answer
   */
  private async waitForAnswer(): Promise<string> {
    return new Promise((resolve, reject) => {
      if (!this.ws) {
        reject('No signaling connection');
        return;
      }

      const handler = (event: MessageEvent) => {
        const msg = JSON.parse(event.data);
        if (msg.type === 'answer') {
          this.ws!.removeEventListener('message', handler);
          resolve(msg.sdp);
        }
      };

      this.ws.addEventListener('message', handler);

      // Timeout after 30 seconds
      setTimeout(() => {
        reject('Answer timeout');
      }, 30000);
    });
  }

  /**
   * Wait for ICE gathering to complete
   */
  private async waitForIceGathering(): Promise<void> {
    if (!this.peerConnection) return;

    return new Promise((resolve) => {
      if (this.peerConnection!.iceGatheringState === 'complete') {
        resolve();
        return;
      }

      const checkState = () => {
        if (this.peerConnection!.iceGatheringState === 'complete') {
          this.peerConnection!.removeEventListener('icegatheringstatechange', checkState);
          resolve();
        }
      };

      this.peerConnection.addEventListener('icegatheringstatechange', checkState);

      // Timeout after 5 seconds
      setTimeout(resolve, 5000);
    });
  }

  /**
   * Update state and notify listener
   */
  private updateState(state: WebRTCState): void {
    if (this.onStateChange) {
      this.onStateChange(state);
    }
  }
}
