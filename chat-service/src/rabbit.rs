use std::sync::Arc;

use crate::queries::DirectMessage;
use crate::write_direct_message;
use lapin::{
    BasicProperties, Channel, Connection,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::FieldTable,
};
use scylla::value::CqlTimestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct MessagePublisher {
    channel: Channel,
    queue_name: String,
}

impl MessagePublisher {
    pub async fn new(connection: &Connection, queue_name: String) -> lapin::Result<Self> {
        let channel = connection.create_channel().await?;

        // Declare the queue
        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        Ok(Self {
            channel,
            queue_name,
        })
    }

    pub async fn publish_message(
        &self,
        message: &SerializableDirectMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payload = serde_json::to_vec(message)?;

        let confirm = self
            .channel
            .basic_publish(
                "",
                &self.queue_name,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default().with_delivery_mode(2), // Make message persistent
            )
            .await?
            .await?;

        match confirm {
            Confirmation::Ack(_) => Ok(()),
            Confirmation::Nack(_) => Err("Message was nacked by RabbitMQ".into()),
            Confirmation::NotRequested => Err("Publisher confirmation was not requested".into()),
        }
    }
}
pub struct MessageConsumer {
    channel: Channel,
    queue_name: String,
    session: Arc<scylla::client::session::Session>,
}

impl MessageConsumer {
    pub async fn new(
        connection: &Connection,
        queue_name: String,
        session: Arc<scylla::client::session::Session>,
    ) -> lapin::Result<Self> {
        let channel = connection.create_channel().await?;

        channel
            .queue_declare(
                &queue_name,
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        // Set QoS to process one message at a time
        // Research more about this......
        channel.basic_qos(1, BasicQosOptions::default()).await?;

        Ok(Self {
            channel,
            queue_name,
            session,
        })
    }

    pub async fn start_consuming(&self) -> lapin::Result<()> {
        let consumer = self
            .channel
            .basic_consume(
                &self.queue_name,
                "dm_consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        println!("Started consuming messages from queue: {}", self.queue_name);

        let session = Arc::clone(&self.session);

        consumer.set_delegate(
            move |delivery: lapin::Result<Option<lapin::message::Delivery>>| {
                let session = Arc::clone(&session);
                async move {
                    if let Ok(Some(delivery)) = delivery {
                        match serde_json::from_slice::<SerializableDirectMessage>(&delivery.data) {
                            Ok(serializable_dm) => {
                                let dm: DirectMessage = serializable_dm.into();

                                match write_direct_message(&session, dm).await {
                                    Ok(()) => {
                                        println!("Successfully wrote message to database");
                                        delivery.ack(BasicAckOptions::default()).await.unwrap();
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to write message to database: {}", e);
                                        // Reject and requeue the message
                                        delivery
                                            .nack(BasicNackOptions {
                                                requeue: true,
                                                ..Default::default()
                                            })
                                            .await
                                            .unwrap();
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to deserialize message: {}", e);
                                // Reject without requeue for malformed messages
                                delivery
                                    .nack(BasicNackOptions {
                                        requeue: false,
                                        ..Default::default()
                                    })
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                }
            },
        );

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableDirectMessage {
    pub conversation_id: String,
    pub message_id: Uuid,
    pub sender_id: i32,
    pub recipient_id: i32,
    pub message_text: String,
    pub created_at: i64,
}

impl From<DirectMessage> for SerializableDirectMessage {
    fn from(dm: DirectMessage) -> Self {
        Self {
            conversation_id: dm.conversation_id,
            message_id: dm.message_id, 
            sender_id: dm.sender_id,
            recipient_id: dm.recipient_id,
            message_text: dm.message_text,
            created_at: dm.created_at.0,
        }
    }
}

impl From<SerializableDirectMessage> for DirectMessage {
    fn from(sdm: SerializableDirectMessage) -> Self {
        Self {
            conversation_id: sdm.conversation_id,
            message_id: sdm.message_id, 
            sender_id: sdm.sender_id,
            recipient_id: sdm.recipient_id,
            message_text: sdm.message_text,
            created_at: CqlTimestamp(sdm.created_at),
        }
    }
}
