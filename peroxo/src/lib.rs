tonic::include_proto!("auth_service");
tonic::include_proto!("chat_service");

pub mod actors;
pub mod auth;
pub mod chat;
pub mod config;
pub mod connections;
mod grpc_auth;
mod handlers;
pub mod metrics;
#[cfg(feature = "mongo_db")]
pub mod mongo_db;
pub mod ratelimit;
mod routes;
pub mod socket;
pub mod startup;
pub mod state;
pub mod telemetry;
pub mod tenant;
pub mod validate;

pub use routes::{metrics_route, peroxo_route};
