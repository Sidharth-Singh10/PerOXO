use axum::{
    Extension, Json,
    extract::ConnectInfo,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::SocketAddr;
use std::sync::Arc;

use tracing::{error, info, warn};

use crate::{db::insert_tenant_keypair, google_auth, rate_limit, redact, tenant, user_token};

/// Maximum accepted identifier length. Ids are opaque strings, but they still
/// must not be able to grow the database or the rate-limit keyspace.
const MAX_ID_LEN: usize = 128;

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

/// Logs the real cause and returns a generic body, so internals (SQL errors,
/// connection strings, dependency URLs) never reach the caller.
fn internal_error(context: &str, detail: &dyn std::fmt::Display) -> (StatusCode, String) {
    error!("{context}: {detail}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal error".to_string(),
    )
}

fn invalid_credentials() -> (StatusCode, String) {
    (StatusCode::UNAUTHORIZED, "Invalid credentials".to_string())
}

/// Trust `X-Forwarded-For` only when the deployment explicitly says a proxy
/// rewrites it. Without the flag the header is spoofable and the rate limiter
/// would key on attacker-chosen values.
fn trust_proxy_headers() -> bool {
    static TRUST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRUST.get_or_init(|| std::env::var("TRUST_PROXY_HEADERS").ok().as_deref() == Some("1"))
}

fn client_ip(peer: SocketAddr, headers: &HeaderMap) -> std::net::IpAddr {
    if trust_proxy_headers()
        && let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .and_then(|value| value.trim().parse().ok())
    {
        return forwarded;
    }

    peer.ip()
}

pub async fn generate_tenant_handler(
    Extension(pool): Extension<PgPool>,
    Extension(redis): Extension<Arc<redis::aio::ConnectionManager>>,
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
            google_auth::GoogleAuthError::InvalidToken(reason) => {
                warn!(%reason, "Google rejected the ID token");
                (StatusCode::UNAUTHORIZED, "Invalid Google token".to_string())
            }
            _ => (
                StatusCode::UNAUTHORIZED,
                "Google authentication failed".to_string(),
            ),
        }
    })?;

    info!(email = %google_user.email, "tenant generation requested");

    rate_limit::check_rate_limit(&redis, &google_user.sub)
        .await
        .map_err(|e| match e {
            rate_limit::RateLimitError::Exceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "Rate limit exceeded: maximum 3 tenant generations per hour".to_string(),
            ),
            rate_limit::RateLimitError::RedisError(re) => {
                internal_error("redis error during tenant rate limit check", &re)
            }
        })?;

    let kp = tenant::TenantKeypair::new();

    let project_id = kp.get_project_id().clone();
    let secret_api_key = kp.get_secret_api_key().clone();

    insert_tenant_keypair(&pool, &project_id, &secret_api_key)
        .await
        .map_err(|e| internal_error("failed to insert tenant keypair", &e))?;

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
    Extension(redis): Extension<Arc<redis::aio::ConnectionManager>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<GenerateUserTokenRequest>,
) -> Result<Json<GenerateUserTokenResponse>, (StatusCode, String)> {
    // Bound the identifiers before they reach the database or the limiter.
    if payload.project_id.is_empty()
        || payload.project_id.len() > MAX_ID_LEN
        || payload.user_id.is_empty()
        || payload.user_id.len() > MAX_ID_LEN
        || payload.secret_api_key.is_empty()
    {
        return Err((StatusCode::BAD_REQUEST, "Invalid request".to_string()));
    }

    // Rate limit by client IP *and* project: the IP bound stops enumeration
    // across projects, the project bound stops distributed guessing against
    // one known project.
    let limiter_subject = format!("{}:{}", client_ip(peer, &headers), payload.project_id);
    rate_limit::check(&redis, rate_limit::GENERATE_USER_TOKEN, &limiter_subject)
        .await
        .map_err(|e| match e {
            rate_limit::RateLimitError::Exceeded => (
                StatusCode::TOO_MANY_REQUESTS,
                "Too many token requests".to_string(),
            ),
            rate_limit::RateLimitError::RedisError(re) => {
                internal_error("redis error during token rate limit check", &re)
            }
        })?;

    info!(project_id = %payload.project_id, "generate_user_token_handler called");

    // Verify the secret_api_key exists in the database.
    let stored_key = crate::db::get_secret_key_by_project_id(&pool, &payload.project_id)
        .await
        .map_err(|e| internal_error("db error while fetching stored key", &e))?;

    let stored_key = match stored_key {
        Some(key) => key,
        None => {
            warn!(project_id = %payload.project_id, "missing secret key for project");
            return Err(invalid_credentials());
        }
    };

    // Constant-time comparison: a byte-by-byte early return leaks how much of
    // the key matched.
    if !redact::constant_time_eq(stored_key.as_bytes(), payload.secret_api_key.as_bytes()) {
        warn!(project_id = %payload.project_id, "secret_api_key mismatch");
        return Err(invalid_credentials());
    }

    // Generate and store the user token in Redis.
    let user_token = user_token::store_user_token(&redis, &payload.project_id, &payload.user_id)
        .await
        .map_err(|e| internal_error("failed to store user token", &e))?;

    let resp = GenerateUserTokenResponse { user_token };

    Ok(Json(resp))
}

pub async fn verify_user_token_handler(
    Extension(redis): Extension<Arc<redis::aio::ConnectionManager>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(token): Json<String>,
) -> Result<Json<Option<user_token::UserToken>>, (StatusCode, String)> {
    if token.is_empty() || token.len() > MAX_ID_LEN {
        return Ok(Json(None));
    }

    rate_limit::check(
        &redis,
        rate_limit::VERIFY_USER_TOKEN,
        &client_ip(peer, &headers).to_string(),
    )
    .await
    .map_err(|e| match e {
        rate_limit::RateLimitError::Exceeded => (
            StatusCode::TOO_MANY_REQUESTS,
            "Too many verification requests".to_string(),
        ),
        rate_limit::RateLimitError::RedisError(re) => {
            internal_error("redis error during verify rate limit check", &re)
        }
    })?;

    match user_token::verify_user_token(&redis, &token).await {
        Ok(result) => {
            info!(found = %result.is_some(), "verify_user_token result");
            Ok(Json(result))
        }
        Err(err) => Err(internal_error("error verifying user token", &err)),
    }
}
