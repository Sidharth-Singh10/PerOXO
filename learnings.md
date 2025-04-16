Broadcast module, which implements a multi-producer, multi-consumer channel.

It’s designed for broadcasting a message to all listeners (subscribers).

So broadcast::Sender<ChatMessage> means:
- You can .send(msg) a message of type ChatMessage.
- Anyone subscribed to that sender (using sender.subscribe()) will receive the message.

