use crate::{GetSertConversationRequest, state::PerOxoState};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ConversationParams {
    pub project_id: String,
    pub user_id_1: String,
    pub user_id_2: String,
}

#[derive(Serialize)]
pub struct ConversationResponse {
    pub success: bool,
    pub error_message: String,
    pub conversation_id: String,
    pub created_new: bool,
}

pub async fn getsert_conversation_id(
    Query(params): Query<ConversationParams>,
    State(state): State<Arc<PerOxoState>>,
) -> impl IntoResponse {
   
    // move this to chat_service
    let (u1, u2) = if params.user_id_1 < params.user_id_2 {
        (params.user_id_1, params.user_id_2)
    } else {
        (params.user_id_2, params.user_id_1)
    };

    let mut client = state.chat_client.clone();
    let request = tonic::Request::new(GetSertConversationRequest {
        project_id: params.project_id,
        user_id_1: u1,
        user_id_2: u2,
    });

    let grpc_response = match client.get_sert_conversation(request).await {
        Ok(resp) => resp.into_inner(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ConversationResponse {
                    success: false,
                    error_message: format!("gRPC error: {}", e),
                    conversation_id: "".to_string(),
                    created_new: false,
                }),
            );
        }
    };

    let response = ConversationResponse {
        success: grpc_response.success,
        error_message: grpc_response.error_message,
        conversation_id: grpc_response.conversation_id,
        created_new: grpc_response.created_new,
    };

    (StatusCode::OK, Json(response))
}
