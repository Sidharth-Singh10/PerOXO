Let’s say Alice sends this message via WebSocket:
```
{
  "DirectMessage": {
    "from": "alice",
    "to": "bob",
    "content": "hi"
  }
}
```
So the **UserSession** :
- receives and parses the JSON.
- sees that the message is a **DirectMessage** to **Bob**.
- does not know **Bob’s** connection channel, nor should it.
- So it forwards the message to the **MessageRouter** via:

`router_sender.send(RouterMessage::SendDirectMessage { from, to, content });`

Now the **MessageRouter** is the kinda like the **central authority**
- Keeps a map of online users `(HashMap<String, Sender<ChatMessage>>)`
- Knows how to deliver messages to any online user
- Can broadcast presence, handle message routing, etc.

Every UserSession only:
- Manages its own WebSocket
- Forwards any outgoing intent to the router
- Receives messages from the router

pretty much,
UserSession = user's personal phone
MessageRouter = the cell tower or router