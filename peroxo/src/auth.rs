use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use std::sync::Arc;
use std::time::Duration;

use crate::{
    UserToken, VerifyUserTokenRequest,
    state::PerOxoState,
    tenant::TenantUserId,
};

pub struct AuthenticatedUser {
    pub user_token: UserToken,
    pub tenant_user_id: TenantUserId,
}

impl FromRequestParts<Arc<PerOxoState>> for AuthenticatedUser {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<PerOxoState>,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_token(parts)?;
        let user_token = verify_token(state, token).await?;
        let tenant_user_id = TenantUserId::from_token(&user_token)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid tenant token".into()))?;

        Ok(Self {
            user_token,
            tenant_user_id,
        })
    }
}

fn extract_token(parts: &Parts) -> Result<String, (StatusCode, String)> {
    if let Some(header) = parts.headers.get("authorization") {
        let value = header
            .to_str()
            .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid authorization header".into()))?;
        if let Some(token) = value.strip_prefix("Bearer ") {
            return Ok(token.to_string());
        }
    }

    let query = parts.uri.query().unwrap_or_default();
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing token".into()))
}

async fn verify_token(
    state: &Arc<PerOxoState>,
    token: String,
) -> Result<UserToken, (StatusCode, String)> {
    let mut client = state.auth_client.clone();
    let req = tonic::Request::new(VerifyUserTokenRequest { token });

    // A hung auth-service must not wedge WebSocket handshakes forever.
    // (Known limitation: if the service was previously unavailable the
    // underlying channel may keep returning a transport error quickly;
    // this timeout guarantees we never wait indefinitely either way.)
    let resp = tokio::time::timeout(Duration::from_secs(5), client.verify_user_token(req))
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "Auth service timeout".to_string()))?
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Auth service error: {e}"),
            )
        })?
        .into_inner();

    if !resp.found {
        return Err((StatusCode::UNAUTHORIZED, "Invalid token".into()));
    }

    resp.user_token.ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Auth service returned found=true but no user_token data".into(),
    ))
}
