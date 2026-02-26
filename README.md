# PerOXO Server

A high-performance, strictly isolated actor-based WebSocket server written in Rust. Designed specifically for handling real-time, high-throughput direct messaging between users at scale. The server meticulously manages WebSocket lifecycles, connection handshakes, real-time message routing, and presence tracking by leveraging Tokio's asynchronous runtime and zero-shared-state message-passing concurrency.

By employing a distributed microservices architecture (Auth Service, Chat Service, User Service, coupled with RabbitMQ Consumers), PerOXO guarantees ultra-low latencies, horizontal scalability, and robust fault tolerance.

## Features

- **Full-duplex WebSocket support** using `axum` and `tokio-tungstenite`
- **User session management** with deeply isolated actor-style components
- **Direct messaging** with sub-millisecond routing latency
- **Concurrency-safe** message processing strictly via `tokio::mpsc` and `oneshot` (No Mutexes)
- **Clean modular microservices** (ConnectionManager, UserSession, MessageRouter, along with standalone Auth & User services)
- **Asynchronous messaging** using RabbitMQ for offloaded, non-blocking background task processing
- **Built-in Observability** with Prometheus metrics and robust Grafana dashboarding

## The Problem it Solves

Modern real-time chat applications face significant challenges when scaling:

1. **The Lock Contention Bottleneck**: Traditional chat architectures often rely on monolithic shared state (e.g., locking a massive `Arc<Mutex<GlobalState>>` to track connections). This creates immense thread contention and latency spikes as concurrent users increase.
2. **Excessive Resource Overheads**: Standard threaded or GC-heavy WebSocket servers consume high memory and CPU allocations per connection, raising server costs significantly.
3. **Synchronous Coupling**: Mixing rapid real-time message routing with slower database operations blocks event loops, severely degrading real-time chat responsiveness.

## Why Our Method is Better

PerOXO elegantly obliterates these bottlenecks by strictly adhering to the **Actor Model** powered by Rust:

- **Zero Lock Contention**: By completely eliminating shared mutable memory in favor of message passing via channels, actors process individual queues sequentially. **There is absolutely no Mutex locking required** to route a message from one user to another.
- **Extreme Hardware Efficiency**: Rust's zero-cost abstractions combined with Tokio's lightweight green tasks allow supporting hundreds of thousands of concurrent WebSocket connections on minimal, commodity hardware. It systematically outperforms garbage-collected runtimes heavily in tail latencies.
- **Fail-Safe Fault Isolation**: If a single `UserSession` actor encounters corrupted data and panics, it uniquely crashes _only_ that user's session task. The `MessageRouter` and all other thousands of active users remain entirely insulated and perfectly uninterrupted.
- **Decoupled Asynchronous Persistence**: Database commits and notifications are instantly offloaded to a standalone `PersistenceActor` and message brokers (RabbitMQ). Real-time message delivery executes immediately with zero blocking on network or disk I/O.

## Architecture Overview

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

## Authentication

if peroxo used as a hosted chat engine as a service (needs improvement)
![auth](https://github.com/user-attachments/assets/31b4c1a0-ab2a-4676-8cd8-a49ec2ffc6d3)

## Notes/References

![actors](https://github.com/user-attachments/assets/484c5066-9b26-47c4-b1e1-62fdb648f483)
