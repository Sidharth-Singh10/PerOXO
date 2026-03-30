# The Actor Model in PerOXO

## What Is the Actor Model

The actor model is a concurrency paradigm where the unit of computation is an
**actor**: a self-contained entity that holds private mutable state and
communicates with other actors exclusively through asynchronous message passing.
Three rules define it:

1. **Exclusive ownership** -- an actor's state is never directly accessible to
   any other actor or task. There are no shared references to it.
2. **Message-driven execution** -- the only way to interact with an actor is to
   send it a message through a channel. The actor processes its inbox
   sequentially, one message at a time.
3. **Fault isolation** -- if an actor panics, only that actor's task crashes.
   All other actors continue operating on their own independent state.

The consequence of these rules is that **mutual exclusion is achieved through
sequential message processing rather than locks**. Because only one message is
ever handled at a time within a single actor, there is no concurrent access to
the actor's fields, and therefore no need for `Mutex`, `RwLock`, `RefCell`, or
any other synchronization primitive around the actor's internal state.

In Rust on Tokio, the pattern is implemented with:

- `tokio::sync::mpsc` -- the actor's mailbox (unbounded or bounded).
- `tokio::sync::oneshot` -- request/response pairs embedded in messages.
- `tokio::spawn` -- each actor runs as a lightweight async task.
- `self` ownership -- the actor takes `mut self` in its `run()` loop, granting
  `&mut` access to all fields without any wrapper types.

---

## Why Actor Model Over Shared Mutable State

A real-time WebSocket server like PerOXO manages three categories of mutable
state that multiple connections need to interact with:

| State | Mutated by | Read by |
|-------|-----------|---------|
| Online user registry | every connect/disconnect | every message route |
| Per-user outbound channel map | session lifecycle | message delivery |
| Room membership map | join/leave/cleanup | every room broadcast |

The naive approach wraps each of these in `Arc<Mutex<T>>` (or `Arc<RwLock<T>>`)
and shares them across all connection handler tasks. This creates several
concrete problems at scale:

### 1. Lock contention scales with connection count

Every WebSocket handler task contends on the same lock. With `N` concurrent
connections, the probability of lock contention grows as O(N). Under the Tokio
multi-threaded runtime, this means worker threads stall waiting for a lock held
by a task that may itself be descheduled -- the classic async-mutex priority
inversion problem.

### 2. Atomic operations constrain the optimizer

As explained by matthieum in the Rust community:

> Apart from the Relaxed memory orderings, atomic operations impose constraints
> on the other memory loads & stores around the operation. In the case of a
> Mutex, where Acquire & Release memory orderings are required, there may be an
> opportunity loss.

Even `std::sync::Mutex` in an uncontended case executes an atomic
compare-and-swap (CAS) with a `lock` prefix on x86, which prevents the CPU from
reordering surrounding loads and stores. The optimizer cannot fold, elide, or
reorder any memory operations across the lock boundary. In a hot loop processing
thousands of messages per second, these invisible constraints accumulate.

### 3. Deadlock risk under composition

When multiple locks must be acquired together (e.g. checking the user map while
also modifying the room map), lock ordering discipline is required to prevent
deadlocks. This discipline is enforced only by convention, not by the type
system, and is a common source of production incidents.

### 4. RefCell is not an option in async multi-threaded code

`Rc<RefCell<T>>` is `!Send`, meaning it cannot exist inside a future that
crosses an `.await` point on a multi-threaded runtime. Tokio's default
`#[tokio::main]` is multi-threaded, so `RefCell` is a non-starter for any state
that persists across awaits -- which is all actor state by definition.

---

## What PerOXO Would Look Like Without the Actor Model

To make the contrast concrete, here is how the user registry and message routing
would look if built with shared mutable state instead of actors.

### Shared-state version (hypothetical)

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

struct SharedState {
    users: Mutex<HashMap<TenantUserId, mpsc::Sender<ChatMessage>>>,
    online_users: Mutex<Vec<TenantUserId>>,
    rooms: Mutex<HashMap<String, Mutex<HashMap<TenantUserId, mpsc::Sender<ChatMessage>>>>>,
}

async fn handle_connection(state: Arc<SharedState>, socket: WebSocket, user_id: TenantUserId) {
    let (tx, mut rx) = mpsc::channel(100);

    // Registration: two locks acquired sequentially
    {
        let mut users = state.users.lock().await;       // LOCK 1
        users.insert(user_id.clone(), tx.clone());
    }
    {
        let mut online = state.online_users.lock().await; // LOCK 2
        online.push(user_id.clone());
    }

    // Message handling: lock acquired on every single incoming message
    while let Some(Ok(Message::Text(text))) = socket.next().await {
        let msg: ChatMessage = serde_json::from_str(&text).unwrap();
        let users = state.users.lock().await;            // LOCK on every message
        if let Some(recipient_tx) = users.get(&msg.to) {
            let _ = recipient_tx.try_send(msg);
        }
    }

    // Cleanup: two more locks
    {
        let mut users = state.users.lock().await;
        users.remove(&user_id);
    }
    {
        let mut online = state.online_users.lock().await;
        online.retain(|u| u != &user_id);
    }
}
```

Problems visible in this code:

- **Every message delivery locks the user map.** With 10,000 concurrent
  connections each sending messages, this is 10,000 tasks contending on a single
  `Mutex<HashMap>`.
- **Room state requires nested locks** -- a `Mutex<HashMap<_, Mutex<HashMap>>>` --
  which is both an ergonomic nightmare and a deadlock risk.
- **Registration and cleanup each acquire two separate locks sequentially,**
  creating a window where state is inconsistent (user in `users` but not in
  `online_users`).
- **Adding persistence** would require passing `Arc<SharedState>` into yet more
  spawned tasks, widening the contention surface.

### Actor version (actual PerOXO code)

The `MessageRouter` owns all of this state privately:

```rust
// peroxo/src/actors/message_router/router.rs
pub struct MessageRouter {
    pub receiver: mpsc::UnboundedReceiver<RouterMessage>,
    pub users: HashMap<TenantUserId, mpsc::Sender<ChatMessage>>,
    pub online_users: Vec<TenantUserId>,
    pub rooms: HashMap<String, mpsc::UnboundedSender<RoomMessage>>,
}
```

No `Arc`, no `Mutex`, no `RefCell`. Plain `HashMap` and `Vec` with direct
`&mut self` access. Registration is a direct insert:

```rust
// peroxo/src/actors/message_router/handlers.rs
pub async fn handle_register_user(
    &mut self,
    tenant_user_id: TenantUserId,
    sender: mpsc::Sender<ChatMessage>,
    respond_to: oneshot::Sender<Result<(), String>>,
) {
    if self.users.contains_key(&tenant_user_id) {
        let _ = respond_to.send(Err("User already online".to_string()));
        return;
    }
    self.users.insert(tenant_user_id.clone(), sender);
    self.online_users.push(tenant_user_id.clone());
    let _ = respond_to.send(Ok(()));
}
```

Message delivery is a direct `HashMap::get`:

```rust
if let Some(recipient_sender) = self.users.get(&to) {
    match recipient_sender.try_send(message) {
        Ok(()) => { /* delivered */ }
        Err(TrySendError::Full(_)) => { /* backpressure */ }
        Err(TrySendError::Closed(_)) => { /* stale connection */ }
    }
}
```

No lock acquisition, no atomic CAS, no memory ordering constraints. The
sequential processing of the actor's `mpsc` receiver guarantees that
`self.users` is never accessed concurrently.

---

## How Each PerOXO Actor Avoids Shared Mutable State

### MessageRouter

The central routing hub. Owns the user map, online list, and room sender map.
Processes `RouterMessage` variants sequentially from a single
`mpsc::UnboundedReceiver`. Other tasks (connection handlers, user sessions)
interact with it only by sending messages through clones of the
`mpsc::UnboundedSender<RouterMessage>`.

State that would require `Arc<Mutex<_>>` in a shared-state design:
- `users: HashMap<TenantUserId, mpsc::Sender<ChatMessage>>` -- plain `HashMap`.
- `online_users: Vec<TenantUserId>` -- plain `Vec`.
- `rooms: HashMap<String, mpsc::UnboundedSender<RoomMessage>>` -- plain `HashMap`.

### UserSession

Per-connection actor. Owns its WebSocket split (sink + stream), its
`session_receiver`, and a reference to the router sender. No mutable state is
shared with any other task. The two spawned subtasks (send loop and receive loop)
communicate through the session's own `mpsc` channel, not through shared memory.

### RoomActor

Per-room actor. Owns the room membership map as a plain `HashMap`. A
`tokio::select!` loop multiplexes message handling and periodic cleanup of stale
members within the same task, eliminating any need to share the members map
across tasks:

```rust
// peroxo/src/actors/room_actor.rs
pub struct RoomActor {
    room_id: String,
    receiver: mpsc::UnboundedReceiver<RoomMessage>,
    members: HashMap<TenantUserId, mpsc::Sender<ChatMessage>>,
}

pub async fn run(mut self) {
    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            message = self.receiver.recv() => {
                match message {
                    Some(msg) => self.handle_message(msg).await,
                    None => break,
                }
            }
            _ = cleanup_interval.tick() => {
                self.members.retain(|_, sender| !sender.is_closed());
            }
        }
    }
}
```

Adding and removing members is a direct `HashMap::insert` / `HashMap::remove`
on `&mut self` -- no locking, no TOCTOU races, no contention.

### PersistenceActor

Owns the gRPC client and processes persistence requests from its unbounded
`mpsc` receiver. Database operations that could block message routing are
completely decoupled -- the router fires a message and optionally awaits a
`oneshot` response, never holding any lock while the database call executes.

---

## Where Arc Is Still Used (And Why That's Fine)

`Arc` appears in PerOXO in two categories, neither of which involves mutable
shared state:

**1. Sharing immutable handles across Axum handlers**

`PerOxoState` is wrapped in `Arc` and passed as Axum `State`:

```rust
// peroxo/src/state.rs
pub struct PerOxoState {
    pub connection_manager: Arc<ConnectionManager>,
    pub router_sender: mpsc::UnboundedSender<RouterMessage>,
    // ...
}
```

`ConnectionManager` is immutable after construction -- it holds only an
`mpsc::UnboundedSender`, which is `Clone + Send + Sync` and internally handles
its own synchronization. The `Arc` here is just reference counting to share this
handle across Axum handler futures. Cost: one atomic increment per handler
invocation, which is negligible compared to the syscalls involved in accepting a
WebSocket connection.

**2. Sharing read-only database resources in chat-service**

```rust
// chat-service/src/chat_services.rs
pub struct ChatServiceImpl {
    session: Arc<Session>,
    queries: Arc<Queries>,
}
```

The Scylla `Session` and prepared `Queries` are initialized once at startup and
never mutated. `Arc` distributes them across gRPC handler tasks. No `Mutex`
wrapping, no contention.

---

## Summary

| Concern | Shared-state approach | Actor approach (PerOXO) |
|---------|----------------------|------------------------|
| User map access | `Arc<Mutex<HashMap>>`, locked on every message | `HashMap` on `&mut self`, zero locks |
| Room membership | `Arc<Mutex<HashMap<_, Mutex<HashMap>>>>` | `HashMap` on `&mut self` per `RoomActor` |
| Cleanup timer | Separate spawned task sharing `Arc<Mutex<_>>` | `tokio::select!` branch in same actor loop |
| Persistence | Holds lock while awaiting DB response, or complex lock-then-spawn dance | Fire-and-forget message to `PersistenceActor`, await `oneshot` reply |
| Adding new state fields | New `Mutex<T>` field, audit all lock orderings | New field on actor struct, zero synchronization changes |
| Failure blast radius | Panic while holding lock poisons the `Mutex` for all tasks | Panic kills one actor task, all others unaffected |
| Optimizer freedom | Atomic ops prevent reordering across lock boundaries | Plain memory operations, full optimizer freedom |
