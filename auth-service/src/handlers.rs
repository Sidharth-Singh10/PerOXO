use axum::{Extension, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{db::insert_tenant_keypair, tenant, user_token};

#[derive(Serialize)]
pub struct GenerateTenantResponse {
    project_id: String,
    secret_api_key: String,
}

#[derive(Deserialize)]
pub struct GenerateUserTokenRequest {
    user_id: String,
    project_id: String,
    secret_api_key: String,
}

#[derive(Serialize)]
pub struct GenerateUserTokenResponse {
    user_token: String,
}

pub async fn generate_tenant_handler(
    Extension(pool): Extension<PgPool>,
) -> Result<Json<GenerateTenantResponse>, (StatusCode, String)> {
    let kp = tenant::TenantKeypair::new();

    let project_id = kp.get_project_id().clone();
    let secret_api_key = kp.get_secret_api_key().clone();

    insert_tenant_keypair(&pool, &project_id, &secret_api_key)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let resp = GenerateTenantResponse {
        project_id,
        secret_api_key,
    };

    Ok(Json(resp))
}

pub async fn generate_user_token_handler(
    Extension((pool, redis_client)): Extension<(PgPool, redis::Client)>,
    Json(payload): Json<GenerateUserTokenRequest>,
) -> Result<Json<GenerateUserTokenResponse>, (StatusCode, String)> {
    // Verify the secret_api_key exists in the database
    let stored_key = crate::db::get_secret_key_by_project_id(&pool, &payload.project_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stored_key = match stored_key {
        Some(key) => key,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Invalid project_id or secret_api_key".to_string(),
            ));
        }
    };

    // Verify that the provided secret_api_key matches the stored one
    if stored_key != payload.secret_api_key {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid secret_api_key".to_string(),
        ));
    }

    // Generate and store the user token in Redis
    let user_token =
        user_token::store_user_token(&redis_client, &payload.project_id, &payload.user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to store token: {}", e),
                )
            })?;

    let resp = GenerateUserTokenResponse { user_token };

    Ok(Json(resp))
}

pub async fn verify_user_token_handler(
    Extension((_, redis_client)): Extension<(PgPool, redis::Client)>,
    Json(payload): Json<String>,
) -> Result<Json<Option<user_token::UserToken>>, (StatusCode, String)> {
    let token = payload;

    match user_token::verify_user_token(&redis_client, &token).await {
        Ok(result) => Ok(Json(result)),
        Err(err) => Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string())),
    }
}
