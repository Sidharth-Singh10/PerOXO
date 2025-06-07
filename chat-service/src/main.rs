use crate::chat_services::ChatServiceImpl;
use crate::migrations::run_database_migrations;
use crate::rabbit::MessagePublisher;
use lapin::Connection;
use lapin::ConnectionProperties;
use scylla::client::session_builder::SessionBuilder;
use scylla::client::{execution_profile::ExecutionProfile, session::Session};
use scylla::statement::Consistency;
use std::error::Error;
use std::sync::Arc;
use tonic::transport::Server;
mod chat_services;
mod queries;
mod rabbit;
use crate::chat_service::chat_service_server::ChatServiceServer;
use crate::rabbit::MessageConsumer;
pub mod chat_service {
    tonic::include_proto!("chat_service");
}
mod migrations;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    //  Scylla session
    run_database_migrations("127.0.0.1:9042").await?;

    let profile = ExecutionProfile::builder()
        .consistency(Consistency::One)
        .build();

    let session: Session = SessionBuilder::new()
        .known_node("127.0.0.1:9042")
        .default_execution_profile_handle(profile.into_handle())
        .build()
        .await?;

    let session_arc = Arc::new(session);
    ///////////////////////////

    // RabbitMQ connections
    let rabbitmq_url = "amqp://localhost:5672";
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default()).await?;

    let queue_name = "direct_messages".to_string();

    let publisher = Arc::new(MessagePublisher::new(&connection, queue_name.clone()).await?);

    let consumer = MessageConsumer::new(&connection, queue_name, Arc::clone(&session_arc)).await?;

    // Start consuming in a separate task
    let _consumer_handle = tokio::spawn(async move {
        if let Err(e) = consumer.start_consuming().await {
            eprintln!("Consumer error: {}", e);
        }
    });

    //////////////////////////////////////

    // Create the gRPC service
    let chat_service = ChatServiceImpl::new(
        Arc::try_unwrap(session_arc).unwrap(), // fix: handle Arc properly
        publisher,
    );
    let service = ChatServiceServer::new(chat_service);

    let addr = "[::1]:50051".parse()?;

    println!("ChatService gRPC server listening on {}", addr);

    // Start the gRPC server
    Server::builder().add_service(service).serve(addr).await?;

    Ok(())
}
