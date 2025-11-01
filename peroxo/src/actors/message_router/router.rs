use super::messages::RouterMessage;
#[cfg(any(feature = "mongo_db", feature = "persistence"))]
use crate::actors::persistance_actor::PersistenceMessage;
use crate::actors::room_actor::RoomMessage;
use crate::chat::ChatMessage;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::info;

pub struct MessageRouter {
    pub receiver: mpsc::UnboundedReceiver<RouterMessage>,
    pub users: HashMap<i32, mpsc::Sender<ChatMessage>>,
    pub online_users: Vec<i32>,
    #[cfg(any(feature = "mongo_db", feature = "persistence"))]
    pub persistence_sender: Option<mpsc::UnboundedSender<PersistenceMessage>>,
    pub rooms: HashMap<String, mpsc::UnboundedSender<RoomMessage>>,
}

impl MessageRouter {
    pub fn new(
        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
        persistence_sender: mpsc::UnboundedSender<PersistenceMessage>,
    ) -> (Self, mpsc::UnboundedSender<RouterMessage>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let router = Self {
            receiver,
            users: HashMap::new(),
            online_users: Vec::new(),
            #[cfg(any(feature = "mongo_db", feature = "persistence"))]
            persistence_sender: Some(persistence_sender),
            rooms: HashMap::new(),
        };

        (router, sender)
    }

    pub async fn run(mut self) {
        info!("Message router started");

        while let Some(message) = self.receiver.recv().await {
            match message {
                RouterMessage::RegisterUser {
                    user_id,
                    sender,
                    respond_to,
                } => {
                    self.handle_register_user(user_id, sender, respond_to).await;
                }
                RouterMessage::UnregisterUser { user_id } => {
                    self.handle_unregister_user(user_id).await;
                }
                RouterMessage::SendDirectMessage {
                    from,
                    to,
                    content,
                    message_id,
                    #[allow(unused_variables)]
                    respond_to,
                } => {
                    self.handle_direct_message(
                        from,
                        to,
                        content,
                        message_id,
                        #[cfg(any(feature = "mongo_db", feature = "persistence"))]
                        respond_to,
                    )
                    .await;
                }
                RouterMessage::GetOnlineUsers { respond_to } => {
                    let _ = respond_to.send(self.online_users.clone());
                }
                #[cfg(any(feature = "mongo_db", feature = "persistence"))]
                RouterMessage::GetPaginatedMessages {
                    message_id,
                    conversation_id,
                    respond_to,
                } => {
                    self.handle_get_paginated_chat_history(message_id, conversation_id, respond_to)
                        .await;
                }
                RouterMessage::JoinRoom {
                    user_id,
                    room_id,
                    sender,
                    respond_to,
                } => {
                    self.handle_join_room(user_id, room_id, sender, respond_to)
                        .await;
                }
                RouterMessage::LeaveRoom { user_id, room_id } => {
                    self.handle_leave_room(user_id, room_id).await;
                }
                RouterMessage::SendRoomMessage {
                    room_id,
                    from,
                    content,
                    message_id,
                    respond_to,
                } => {
                    self.handle_room_message(room_id, from, content, message_id, respond_to)
                        .await;
                }
                RouterMessage::GetRoomMembers {
                    room_id,
                    respond_to,
                } => {
                    self.handle_get_room_members(room_id, respond_to).await;
                }
                #[cfg(feature = "persistence")]
                RouterMessage::SyncMessages {
                    conversation_id,
                    message_id,
                    respond_to,
                } => {
                    self.handle_sync_messages(conversation_id, message_id, respond_to)
                        .await;
                }
            }
        }

        info!("Message router stopped");
    }
}
