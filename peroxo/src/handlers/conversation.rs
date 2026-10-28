use crate::{GetSertConversationRequest, auth::AuthenticatedUser, state::PerOxoState};
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
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    // The conversation must belong to the authenticated tenant: a caller
    // holding a token for project A must not be able to touch project B's
    // conversations.
    if params.project_id != auth.user_token.project_id {
        return (
            StatusCode::FORBIDDEN,
            Json(ConversationResponse {
                success: false,
                error_message: "project_id does not match authenticated tenant".to_string(),
                conversation_id: String::new(),
                created_new: false,
            }),
        );
    }

    // Only a participant may ask for a conversation id. Without this, any
    // tenant member could enumerate the conversation ids of any other pair in
    // the project.
    if auth.user_token.user_id != params.user_id_1 && auth.user_token.user_id != params.user_id_2 {
        return (
            StatusCode::FORBIDDEN,
            Json(ConversationResponse {
                success: false,
                error_message: "caller is not a participant of this conversation".to_string(),
                conversation_id: String::new(),
                created_new: false,
            }),
        );
    }

    // Bound the identifiers before they reach the database.
    if params.user_id_1.is_empty()
        || params.user_id_2.is_empty()
        || params.user_id_1.len() > 128
        || params.user_id_2.len() > 128
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConversationResponse {
                success: false,
                error_message: "invalid user id".to_string(),
                conversation_id: String::new(),
                created_new: false,
            }),
        );
    }

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
        caller_user_id: auth.user_token.user_id.clone(),
    });

    let grpc_response = match client.get_sert_conversation(request).await {
        Ok(resp) => resp.into_inner(),
        Err(e) => {
            tracing::error!("get_sert_conversation gRPC call failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ConversationResponse {
                    success: false,
                    error_message: "Internal error".to_string(),
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
