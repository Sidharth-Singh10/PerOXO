use axum::{
    Extension, Json,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use tracing::{error, info, warn};

use crate::{db::insert_tenant_keypair, google_auth, rate_limit, tenant, user_token};

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
    Extension(redis_client): Extension<redis::Client>,
    headers: HeaderMap,
) -> Result<Json<GenerateTenantResponse>, (StatusCode, String)> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            warn!("generate-tenant called without valid Authorization header");
            (
                StatusCode::UNAUTHORIZED,
                "Missing or invalid Authorization header".to_string(),
            )
        })?;

    let google_user = google_auth::verify_id_token(token).await.map_err(|e| {
        warn!(?e, "Google token verification failed");
        match e {
            google_auth::GoogleAuthError::AudienceMismatch => (
                StatusCode::UNAUTHORIZED,
                "Token audience mismatch".to_string(),
            ),
            google_auth::GoogleAuthError::InvalidToken(msg) => {
                (StatusCode::UNAUTHORIZED, format!("Invalid Google token: {msg}"))
            }
            _ => (
                StatusCode::UNAUTHORIZED,
                "Google authentication failed".to_string(),
            ),
        }
    })?;

    info!(email = %google_user.email, "tenant generation requested");

    rate_limit::check_rate_limit(&redis_client, &google_user.sub).map_err(|e| match e {
        rate_limit::RateLimitError::Exceeded => (
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded: maximum 3 tenant generations per hour".to_string(),
        ),
        rate_limit::RateLimitError::RedisError(re) => {
            error!(%re, "redis error during rate limit check");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal error".to_string(),
            )
        }
    })?;

    let kp = tenant::TenantKeypair::new();

    let project_id = kp.get_project_id().clone();
    let secret_api_key = kp.get_secret_api_key().clone();

    if let Err(e) = insert_tenant_keypair(&pool, &project_id, &secret_api_key).await {
        error!(%e, "failed to insert tenant keypair");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    info!(
        email = %google_user.email,
        project_id = %project_id,
        "tenant keypair generated"
    );

    let resp = GenerateTenantResponse {
        project_id,
        secret_api_key,
    };

    Ok(Json(resp))
}

pub async fn generate_user_token_handler(
    Extension(pool): Extension<PgPool>,
    Extension(redis_client): Extension<redis::Client>,
    Json(payload): Json<GenerateUserTokenRequest>,
) -> Result<Json<GenerateUserTokenResponse>, (StatusCode, String)> {
    info!(project_id = %payload.project_id, user_id = %payload.user_id, "generate_user_token_handler called");

    // Verify the secret_api_key exists in the database
    let stored_key = crate::db::get_secret_key_by_project_id(&pool, &payload.project_id)
        .await
        .map_err(|e| {
            error!(%e, "db error while fetching stored key");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    let stored_key = match stored_key {
        Some(key) => key,
        None => {
            warn!(project_id = %payload.project_id, "invalid project_id or missing secret key");
            return Err((
                StatusCode::UNAUTHORIZED,
                "Invalid project_id or secret_api_key".to_string(),
            ));
        }
    };

    // Verify that the provided secret_api_key matches the stored one
    if stored_key != payload.secret_api_key {
        warn!(project_id = %payload.project_id, "secret_api_key mismatch");
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
                error!(%e, "failed to store user token");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to store token: {}", e),
                )
            })?;

    let resp = GenerateUserTokenResponse { user_token };

    Ok(Json(resp))
}

pub async fn verify_user_token_handler(
    Extension(redis_client): Extension<redis::Client>,
    Json(payload): Json<String>,
) -> Result<Json<Option<user_token::UserToken>>, (StatusCode, String)> {
    let token = payload;

    match user_token::verify_user_token(&redis_client, &token).await {
        Ok(result) => {
            info!(found = %result.is_some(), "verify_user_token result");
            Ok(Json(result))
        }
        Err(err) => {
            error!(%err, "error verifying user token");
            Err((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
        }
    }
}
