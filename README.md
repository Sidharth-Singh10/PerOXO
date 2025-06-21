# PerOXO Server

An actor-based WebSocket server written in Rust for handling real-time direct messaging between users. This server manages WebSocket connections, handles user registration and deregistration, routes messages, and tracks user presence efficiently using Tokio's async and message-passing capabilities.


## 📦 Features

- **Full-duplex WebSocket support** using `axum` and `tokio-tungstenite`
- **User session management** with actor-style components
- **Direct messaging** between users
- **Concurrency-safe** message routing using `tokio::mpsc` and `oneshot`
- **Clean modular architecture** (ConnectionManager, UserSession, MessageRouter)

## 🛠️ Architecture Overview

```
Client
  ↓ WebSocket
dm_socket (Axum Handler)
  ↓
ConnectionManager
  ↓ initializes
UserSession (per user)
  ↓ register + send/receive
MessageRouter (central actor)
  ↑ server-to-client messages
```


## Overall Architecture (In development)
![Screenshot_20250618_112813](https://github.com/user-attachments/assets/2bcc5594-a087-4a7f-8f29-5f66976a6968)

