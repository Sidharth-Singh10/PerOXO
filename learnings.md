Broadcast module, which implements a multi-producer, multi-consumer channel.

It’s designed for broadcasting a message to all listeners (subscribers).

So broadcast::Sender<ChatMessage> means:
- You can .send(msg) a message of type ChatMessage.
- Anyone subscribed to that sender (using sender.subscribe()) will receive the message.

Protocol Buffers (protobuf) is a language-neutral, platform-neutral, extensible mechanism developed by Google for serializing structured data—think of it like a faster, smaller alternative to XML or JSON.

It lets you define the structure of your data once using a special .proto file, and then it generates code in your language of choice (like Rust, Python, Go, Java, etc.) that can serialize and deserialize that data efficiently.