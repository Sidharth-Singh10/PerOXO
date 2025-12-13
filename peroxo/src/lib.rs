use axum::{
    Router,
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{any, get},
};
use std::{collections::HashMap, sync::Arc};

use crate::{
    metrics::{metrics_handler, metrics_middleware},
    socket::dm_socket,
    state::PerOxoState,
};

tonic::include_proto!("auth_service");

pub mod actors;
pub mod chat;
pub mod connections;
pub mod metrics;
#[cfg(feature = "mongo_db")]
pub mod mongo_db;
pub mod socket;
pub mod state;

async fn verify_token(
    state: &Arc<PerOxoState>,
    token: String,
) -> Result<UserToken, (StatusCode, String)> {
    let mut client = state.auth_client.clone();

    let req = tonic::Request::new(VerifyUserTokenRequest { token });

    let resp = match client.verify_user_token(req).await {
        Ok(r) => r.into_inner(),
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Auth service error: {}", e),
            ));
        }
    };

    if !resp.found {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".to_string()));
    }

    match resp.user_token {
        Some(user_token) => Ok(user_token),
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Auth service returned found=true but no user_token data".to_string(),
        )),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<PerOxoState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token = match params.get("token") {
        Some(token) => token.clone(),
        None => return (StatusCode::BAD_REQUEST, "Missing token").into_response(),
    };

    let user_token = match verify_token(&state, token.clone()).await {
        Ok(id) => id,
        Err(err) => return err.into_response(),
    };

    ws.on_upgrade(move |socket| dm_socket(socket, user_token, state))
}

pub fn peroxo_route(state: Arc<PerOxoState>) -> Router {
    Router::new()
        .route("/ws", any(ws_handler))
        .route("/metrics", get(metrics_handler))
        .layer(middleware::from_fn(metrics_middleware))
        .with_state(state)
}
