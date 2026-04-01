// Claude P2P Remote - Mobile App Frontend

import { WebRTCConnection } from './webrtc';

// DOM Elements
const statusBar = document.getElementById('statusBar') as HTMLElement;
const pairingSection = document.getElementById('pairingSection') as HTMLElement;
const chatSection = document.getElementById('chatSection') as HTMLElement;
const chatContainer = document.getElementById('chatContainer') as HTMLElement;
const messageInput = document.getElementById('messageInput') as HTMLInputElement;
const sendBtn = document.getElementById('sendBtn') as HTMLButtonElement;
const pairingCodeInput = document.getElementById('pairingCode') as HTMLInputElement;
const connectBtn = document.getElementById('connectBtn') as HTMLButtonElement;

// State
let webrtc: WebRTCConnection | null = null;
let isConnected = false;

// Signaling server URL (configure for your deployment)
const SIGNALING_URL = 'ws://localhost:8080/ws';

/**
 * Add a message to the chat UI
 */
function addMessage(text: string, isUser: boolean): void {
  const messageDiv = document.createElement('div');
  messageDiv.className = `message ${isUser ? 'user' : 'claude'}`;
  messageDiv.textContent = text;
  chatContainer.appendChild(messageDiv);
  chatContainer.scrollTop = chatContainer.scrollHeight;
}

/**
 * Update connection status
 */
function updateStatus(status: string, connected: boolean): void {
  statusBar.textContent = status;
  statusBar.classList.toggle('connected', connected);
}

/**
 * Show chat section, hide pairing section
 */
function showChat(): void {
  pairingSection.style.display = 'none';
  chatSection.style.display = 'flex';
}

/**
 * Handle incoming message from WebRTC
 */
function handleIncomingMessage(data: ArrayBuffer): void {
  try {
    const text = new TextDecoder().decode(data);
    const msg = JSON.parse(text);

    console.log('Received message:', msg);

    switch (msg.type) {
      case 'chat_chunk':
        // Append streaming response
        const existingLast = chatContainer.lastElementChild;
        if (existingLast && existingLast.classList.contains('claude')) {
          existingLast.textContent += msg.text;
        } else {
          addMessage(msg.text, false);
        }
        break;

      case 'chat_done':
        console.log('Chat done:', msg.conversation_id);
        break;

      case 'file_list':
        // Handle file listing
        console.log('Files:', msg.entries);
        break;

      case 'file_content':
        // Handle file content
        console.log('File content:', msg.content);
        break;

      case 'error':
        addMessage(`Error: ${msg.message}`, false);
        break;

      case 'need_ack':
        // User confirmation needed
        if (confirm(msg.prompt)) {
          sendAck(true);
        } else {
          sendAck(false);
        }
        break;
    }
  } catch (e) {
    console.error('Failed to parse message:', e);
  }
}

/**
 * Send a chat message
 */
function sendMessage(text: string): void {
  if (!webrtc || !text.trim()) return;

  const msg = {
    type: 'chat',
    message: text,
    conversation_id: null as string | null,
  };

  const data = new TextEncoder().encode(JSON.stringify(msg));
  webrtc.send(data);
  addMessage(text, true);
  messageInput.value = '';
}

/**
 * Send acknowledgment
 */
function sendAck(approved: boolean): void {
  if (!webrtc) return;

  const msg = {
    type: 'ack',
    approved,
  };

  const data = new TextEncoder().encode(JSON.stringify(msg));
  webrtc.send(data);
}

/**
 * Connect to daemon with pairing code
 */
async function connectWithPairingCode(code: string): Promise<void> {
  try {
    updateStatus('Connecting...', false);

    webrtc = new WebRTCConnection(SIGNALING_URL);

    // Set up message handler
    webrtc.onMessage = (data) => handleIncomingMessage(data);

    // Set up state change handler
    webrtc.onStateChange = (state) => {
      console.log('WebRTC state:', state);
      switch (state) {
        case 'connected':
          isConnected = true;
          updateStatus('Connected', true);
          showChat();
          break;
        case 'disconnected':
        case 'failed':
          isConnected = false;
          updateStatus('Disconnected', false);
          break;
        case 'connecting':
          updateStatus('Connecting...', false);
          break;
      }
    };

    // Start connection with pairing code
    await webrtc.connect(code);
  } catch (e) {
    console.error('Connection failed:', e);
    updateStatus('Connection failed', false);
    alert(`Failed to connect: ${e}`);
  }
}

// Event listeners
connectBtn.addEventListener('click', () => {
  const code = pairingCodeInput.value.trim();
  if (code.length !== 6) {
    alert('Please enter a 6-digit pairing code');
    return;
  }
  connectWithPairingCode(code);
});

sendBtn.addEventListener('click', () => {
  sendMessage(messageInput.value);
});

messageInput.addEventListener('keypress', (e) => {
  if (e.key === 'Enter') {
    sendMessage(messageInput.value);
  }
});

// Initialize
updateStatus('Ready to connect', false);
