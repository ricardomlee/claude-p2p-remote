# P2P Claude Code - 架构设计

## Context

用户需求：通过移动端 App 远程连接电脑/服务器上的 Claude Code，实现移动编码辅助。

**核心约束**：
- 无官方 Rust SDK，必须与 Claude Code CLI 通过 PTY 交互
- Daemon spawn 一个 claude 进程，多 Client 复用
- 确认策略：可配置（启动时选择，运行时可切换）
- 技术栈：Rust + Tauri v2（移动端），追求极致性能和简洁代码
- P2P 连接：WebRTC DataChannel（原生支持音视频/图片/视频传输）
- 信令：混合模式（WebSocket 仅交换 SDP）
- 多模态支持：预留语音、图片、视频输入功能（WebRTC 原生支持，部分模型可输入）

**扩展需求**：
- 多模态输入：语音（转文字）、图片、视频
- WebRTC 原生支持媒体流，无需额外中继

---

## 架构设计

### 系统组件

```
┌─────────────────────────────────────────┐              ┌─────────────────┐
│           移动端 App (Tauri v2)          │              │   电脑/服务器   │
│                                         │              │   (Host)        │
│  ┌───────────┐  ┌───────────────────┐   │              │                 │
│  │ WebRTC    │  │ UI 界面           │   │              │  ┌───────────┐  │
│  │ DataChan  │  │ - 对话 (文字/语音)│   │              │  │ Daemon    │  │
│  │ + Media   │  │ - 文件浏览        │   │              │  │ (Rust)    │  │
│  │           │  │ - 图片/视频预览   │   │              │  │           │  │
│  └───────────┘  └───────────────────┘   │              │  │  ┌─────┐  │  │
│                │                         │              │  │  │PTY  │  │  │
│  ┌───────────┐  │                         │              │  │  │     │  │  │
│  │ 媒体输入  │  │                         │              │  │  │Claude│  │  │
│  │ - 麦克风  │──┘                         │              │  │  └─────┘  │  │
│  │ - 相机    │                            │              │  └───────────┘  │
│  └───────────┘                            │              └─────────────────┘
└───────────────────────────────────────────┘
```

**媒体流处理**：
- WebRTC 原生支持音频/视频轨道（`RTCTrackEvent`）
- 语音 → WebRTC 音频轨道 → Daemon 收集 → 语音转文字（可选）→ Claude
- 图片/视频 → WebRTC 数据通道或视频轨道 → Daemon → 多模态模型处理

### 模块划分

```
daemon/
├── src/
│   ├── main.rs           # 入口：解析参数、启动服务
│   ├── lib.rs            # 库导出
│   │
│   ├── protocol/         # 协议层
│   │   ├── mod.rs
│   │   ├── message.rs    # ClientMessage / ServerMessage 枚举
│   │   └── codec.rs      # JSON 序列化/反序列化
│   │
│   ├── webrtc/           # 连接层
│   │   ├── mod.rs
│   │   ├── connection.rs # WebRTC 连接管理
│   │   ├── signaling.rs  # 信令客户端
│   │   └── media.rs      # 媒体轨道处理（音频/视频）
│   │
│   ├── session/          # 会话层
│   │   ├── mod.rs
│   │   ├── manager.rs    # 会话管理器（多路复用）
│   │   └── claude.rs     # Claude CLI 封装 (PTY)
│   │
│   ├── media/            # 媒体处理
│   │   ├── mod.rs
│   │   ├── audio.rs      # 语音采集/转文字
│   │   ├── image.rs      # 图片处理
│   │   └── video.rs      # 视频处理
│   │
│   ├── fs/               # 文件服务
│   │   ├── mod.rs
│   │   └── service.rs    # 文件浏览/读取
│   │
│   └── config/           # 配置层
│       ├── mod.rs
│       └── auth.rs       # 配对码、认证、确认模式
│
├── mobile-app/           # Tauri v2 移动端
│   ├── src/
│   │   ├── main.ts
│   │   ├── webrtc.ts     # WebRTC 连接 + 媒体轨道
│   │   ├── protocol.ts   # 消息协议
│   │   ├── media/        # 媒体输入（麦克风/相机）
│   │   └── ui/           # UI 组件
│   └── src-tauri/        # Tauri 后端（Rust）
│
└── signaling/            # 信令服务器
    └── src/
        └── main.rs       # WebSocket 服务器
```

---

## 核心类型定义

```rust
// protocol/message.rs

/// 客户端 -> 服务端消息
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Chat { message: String },
    ChatWithMedia { message: String, media: MediaRef },  // 带媒体的消息
    FileList { path: String },
    FileRead { path: String },
    SetConfirmMode { mode: ConfirmMode },  // 运行时切换确认模式
    Ack,  // 确认继续
}

/// 媒体引用（通过 WebRTC 轨道传输，此处仅引用）
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MediaRef {
    pub id: String,
    pub ty: MediaType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Audio,   // 语音（可转文字）
    Image,   // 图片
    Video,   // 视频
}

/// 确认模式
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmMode {
    Auto,    // 自动确认所有
    Manual,  // 转发到客户端确认
}

/// 服务端 -> 客户端消息
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    ChatChunk { text: String },
    ChatDone,
    FileList { entries: Vec<FileEntry> },
    FileContent { content: String },
    Error { message: String },
    NeedAck { prompt: String },  // 需要用户确认
}
```

---

## 会话多路复用设计

```rust
// session/manager.rs

/// 会话管理器 - 多 Client 复用一个 Claude 进程
pub struct SessionManager {
    /// 当前活跃的客户端连接
    clients: DashMap<ClientId, ClientSender>,

    /// 共享的 Claude PTY 进程
    claude_pty: Arc<Mutex<ClaudePty>>,

    /// 确认模式（全局，可运行时切换）
    confirm_mode: RwLock<ConfirmMode>,

    /// 当前等待确认的客户端（如果有）
    pending_ack: Option<ClientId>,
}

impl SessionManager {
    /// 添加新客户端
    pub fn add_client(&self, id: ClientId, sender: ClientSender) {
        self.clients.insert(id, sender);
    }

    /// 客户端消息路由
    pub async fn route_message(&self, client_id: ClientId, msg: ClientMessage) {
        // 如果有其他客户端正在等待确认，拒绝新请求
        if let Some(pending) = self.pending_ack {
            if pending != client_id {
                self.send_error(client_id, "等待其他客户端确认中").await;
                return;
            }
        }

        match msg {
            ClientMessage::Chat { message } => {
                self.send_to_claude(&message).await;
                // 流式响应广播给所有客户端（或仅发送者）
            }
            ClientMessage::SetConfirmMode { mode } => {
                *self.confirm_mode.write().await = mode;
            }
            ClientMessage::Ack => {
                self.pending_ack = None;
                // 继续执行
            }
        }
    }
}
```

---

## 关键技术选型 (最新稳定版本)

| 组件 | 库 | 最新版本 | 理由 |
|------|-----|----------|------|
| Async Runtime | tokio | 1.50.0 | Rust 标准异步运行时 |
| WebRTC | webrtc | 0.20.0-alpha.1 | 纯 Rust WebRTC 实现，支持媒体轨道 |
| 信令 | tokio-tungstenite | 0.29.0 | WebSocket 客户端，最新稳定版 |
| PTY | shellwords | 1.1.0 | 命令行解析 + tokio::process 实现伪终端 |
| 并发 Map | dashmap | 6.1.0 | 无锁并发 HashMap(7.x 为 RC 版) |
| 序列化 | serde + serde_json | 1.0.228 | 标准方案 |
| 日志 | tracing | 0.1.44 | 结构化日志 |
| 错误处理 | thiserror | 2.0.18 | 类型安全错误 |
| 移动端框架 | Tauri | 2.10.3 | Rust 生态，轻量安全，支持 iOS/Android |
| 字节处理 | bytes | 1.11.1 | 零拷贝网络 IO |
| UUID | uuid | 1.x | 唯一标识符 |
| 路径处理 | camino | 1.2.2 | UTF-8 路径 |
| Futures | futures | 0.3.32 | 异步工具流 |

**注意**:
- `webrtc 0.20.0-alpha.1` 是 alpha 版本，但这是目前 Rust 生态最完整的 WebRTC 实现
- `dashmap 7.0.0-rc2` 是 rc 版本，建议使用稳定的 6.x 版本
- Tauri v2 已稳定支持移动端（iOS/Android）
- PTY 方案：使用 `shellwords` + `tokio::process` 或 `serial2` 的 PTY 功能

---

## 实施计划

### Phase 1: 项目骨架
- [ ] 创建 Cargo workspace（daemon + signaling）
- [ ] 配置依赖
- [ ] 定义 ProtocolMessage 类型
- [ ] 创建 Tauri v2 移动端项目骨架

### Phase 2: Claude PTY 封装
- [ ] 使用 portable-pty spawn claude 进程
- [ ] 实现 stdin/stdout 通信
- [ ] 处理流式输出解析
- [ ] 解析确认提示

### Phase 3: WebRTC + 信令
- [ ] 实现信令 WebSocket 客户端
- [ ] 实现 WebRTC 连接建立
- [ ] 实现 DataChannel 消息收发
- [ ] 实现媒体轨道处理（音频/视频）

### Phase 4: 会话管理
- [ ] 实现 SessionManager
- [ ] 多客户端并发支持
- [ ] 确认模式切换

### Phase 5: 媒体处理
- [ ] 音频流处理（语音转文字可选）
- [ ] 图片传输处理
- [ ] 视频流处理
- [ ] 与 Claude 多模态模型集成

### Phase 6: 文件服务
- [ ] 目录浏览
- [ ] 文件读取
- [ ] 缓存优化

### Phase 7: 信令服务器
- [ ] 简易 WebSocket 服务器
- [ ] 配对码验证
- [ ] Offer/Answer 转发

### Phase 8: 移动端 UI
- [ ] 对话界面（文字 + 媒体）
- [ ] 文件浏览器
- [ ] 媒体输入控件（麦克风/相机）
- [ ] 确认模式切换

---

## 验证方案

1. 本地启动 daemon（`--confirm-mode auto`）
2. 启动信令服务器
3. Tauri 移动端配对连接
4. 发送文字消息验证 Claude 响应
5. 测试确认模式切换
6. 测试多客户端并发
7. 测试语音输入（语音转文字）
8. 测试图片/视频输入
9. 验证多模态 Claude 响应
