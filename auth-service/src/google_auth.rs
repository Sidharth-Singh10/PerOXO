use serde::Deserialize;
use tracing::{error, instrument, warn};

#[derive(Debug)]
pub struct GoogleUser {
    pub sub: String,
    pub email: String,
}

#[derive(Debug)]
pub enum GoogleAuthError {
    InvalidToken(String),
    AudienceMismatch,
    RequestFailed,
}

#[derive(Deserialize)]
struct TokenInfoResponse {
    sub: String,
    email: String,
    aud: String,
}

/// Verifies a Google ID token by calling Google's tokeninfo endpoint.
/// Returns the authenticated user's `sub` (stable Google user ID) and `email`.
#[instrument(skip(id_token))]
pub async fn verify_id_token(id_token: &str) -> Result<GoogleUser, GoogleAuthError> {
    let url = format!(
        "https://oauth2.googleapis.com/tokeninfo?id_token={}",
        id_token
    );

    let response = reqwest::get(&url).await.map_err(|e| {
        error!(%e, "failed to reach Google tokeninfo endpoint");
        GoogleAuthError::RequestFailed
    })?;

    if !response.status().is_success() {
        warn!(status = %response.status(), "Google tokeninfo returned non-200");
        return Err(GoogleAuthError::InvalidToken(
            "token rejected by Google".to_string(),
        ));
    }

    let info: TokenInfoResponse = response.json().await.map_err(|e| {
        error!(%e, "failed to deserialize tokeninfo response");
        GoogleAuthError::InvalidToken(e.to_string())
    })?;

    let expected_client_id = std::env::var("GOOGLE_CLIENT_ID").map_err(|_| {
        error!("GOOGLE_CLIENT_ID env var not set");
        GoogleAuthError::RequestFailed
    })?;

    if info.aud != expected_client_id {
        warn!(
            expected = %expected_client_id,
            actual = %info.aud,
            "audience mismatch"
        );
        return Err(GoogleAuthError::AudienceMismatch);
    }

    Ok(GoogleUser {
        sub: info.sub,
        email: info.email,
    })
}
