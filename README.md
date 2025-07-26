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


## How the Actor Model is Implemented

### 1. MessageRouter Actor

- **Purpose:** Central hub for routing messages between users
- **State:** Maintains HashMap<i32, mpsc::Sender<ChatMessage>> for active users and online user list
- **Message Types:** RegisterUser, UnregisterUser, SendDirectMessage, GetOnlineUsers, GetChatHistory
- **Runs:** Continuous loop processing messages from mpsc::UnboundedReceiver<RouterMessage>

### 2. UserSession Actor

- **Purpose:** Manages individual WebSocket connections
- **State:** User ID, WebSocket connection, channels for communication
- **Responsibilities:** Handles incoming WebSocket messages, sends outgoing messages, manages connection lifecycle
- **Runs:** Two concurrent tasks - one for sending, one for receiving

### 3. PersistenceActor

- **Purpose:** Handles database persistence via gRPC calls
- **State:** gRPC client connection
- **Message Types:** PersistDirectMessage, GetPaginatedMessages
- **Runs:** Processes persistence requests with retry logic

## Lock Elimination
### 1. Message Passing Instead of Shared State

- No shared mutable data structures
- Each actor owns its state exclusively
- Communication only through channels (mpsc::channel, oneshot::channel)

### 2.Actor Isolation
- Each actor runs in its own task with private state
- No direct memory sharing between actors
- Tokio's async runtime handles task scheduling

## Notes/References

![actors](https://github.com/user-attachments/assets/484c5066-9b26-47c4-b1e1-62fdb648f483)

