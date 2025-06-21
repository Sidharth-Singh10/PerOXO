#  Component Details

##  ConnectionManager
- Accepts incoming WebSocket connections.
- Initializes UserSession and hands off socket + router sender.
- Lives as part of AppState.

##  UserSession
- Registers with the router via `RouterMessage::RegisterUser`.
- Spawns two async tasks:
    - Sending: Reads ChatMessage from router and sends via WebSocket.
    - Receiving: Parses client messages and forwards them to the router.

- On disconnect, sends UnregisterUser to router.

## MessageRouter
- Maintains:
    - users: HashMap<username, sender-to-session>
    - online_users: Vec<String>
- Routes:
    - SendDirectMessage
    - PresenceStatus updates

- Responds to GetOnlineUsers via oneshot::Sender

## RouterMessage
```rs
pub enum RouterMessage {
    RegisterUser { username, sender, respond_to },
    UnregisterUser { username },
    SendDirectMessage { from, to, content },
    GetOnlineUsers { respond_to },
}
```
## ChatMessage
```rs
pub enum ChatMessage {
    DirectMessage { from, to, content },
    Presence { user, status: PresenceStatus },
}
```