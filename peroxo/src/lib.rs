tonic::include_proto!("auth_service");
tonic::include_proto!("chat_service");

pub mod auth;
pub mod actors;
pub mod chat;
pub mod config;
pub mod connections;
mod handlers;
pub mod metrics;
#[cfg(feature = "mongo_db")]
pub mod mongo_db;
mod routes;
pub mod socket;
pub mod startup;
pub mod state;
pub mod telemetry;
pub mod tenant;

pub use routes::peroxo_route;
