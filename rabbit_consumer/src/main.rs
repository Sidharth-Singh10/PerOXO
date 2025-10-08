use lapin::{Connection, ConnectionProperties};
use scylla::{
    client::{
        execution_profile::ExecutionProfile, session::Session, session_builder::SessionBuilder,
    },
    statement::Consistency,
};
use std::env;
use tracing::info;

use crate::{direct_message::DirectMessageHandler, message_handler::MessageConsumer};
mod direct_message;
mod message_handler;
mod room;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting DM Consumer");

    // Get configuration from environment variables
    let rabbitmq_url =
        env::var("RABBITMQ_URL").unwrap_or_else(|_| "amqp://localhost:5672".to_string());
    let scylla_hosts = env::var("SCYLLA_HOST").unwrap_or_else(|_| "127.0.0.1:9042".to_string());
    let queue_name = env::var("QUEUE_NAME").unwrap_or_else(|_| "direct_messages".to_string());

    info!("Connecting to RabbitMQ at: {}", rabbitmq_url);
    info!("Connecting to ScyllaDB at: {}", scylla_hosts);
    info!("Consuming from queue: {}", queue_name);

    let rabbitmq_connection =
        Connection::connect(&rabbitmq_url, ConnectionProperties::default()).await?;
    info!("Connected to RabbitMQ");

    let profile = ExecutionProfile::builder()
        .consistency(Consistency::One)
        .build();

    println!("Connecting to ScyllaDB...");
    let scylla_session: Session = SessionBuilder::new()
        .known_node(&scylla_hosts)
        .default_execution_profile_handle(profile.into_handle())
        .build()
        .await?;

    info!("Connected to ScyllaDB");

    let dm_handler = DirectMessageHandler {
        session: &scylla_session,
    };
    let room_handler = room::RoomMessageHandler {
        session: &scylla_session,
    };

    // Create consumer
    let dm_consumer = MessageConsumer::new(
        &rabbitmq_connection,
        "direct_messages".to_string(),
        dm_handler,
    )
    .await?;

    let room_consumer = MessageConsumer::new(
        &rabbitmq_connection,
        "room_messages".to_string(),
        room_handler,
    )
    .await?;

    let _ = tokio::join!(
        dm_consumer.start_consuming(),
        room_consumer.start_consuming(),
    );

    Ok(())
}
