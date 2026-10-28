use serde::Deserialize;
use std::time::Duration;
use tracing::{error, instrument, warn};

const TOKENINFO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct GoogleUser {
    pub sub: String,
    pub email: String,
}

#[derive(Debug)]
pub enum GoogleAuthError {
    InvalidToken(String),
    AudienceMismatch,
    EmailNotVerified,
    RequestFailed,
}

#[derive(Deserialize)]
struct TokenInfoResponse {
    sub: String,
    email: String,
    aud: String,
    /// Google returns this as either a boolean or the string "true" depending
    /// on the endpoint version, so accept both.
    #[serde(default)]
    email_verified: Option<serde_json::Value>,
}

impl TokenInfoResponse {
    fn email_is_verified(&self) -> bool {
        match &self.email_verified {
            Some(serde_json::Value::Bool(value)) => *value,
            Some(serde_json::Value::String(value)) => value == "true",
            _ => false,
        }
    }
}

/// Verifies a Google ID token by calling Google's tokeninfo endpoint.
/// Returns the authenticated user's `sub` (stable Google user ID) and `email`.
#[instrument(skip(id_token))]
pub async fn verify_id_token(id_token: &str) -> Result<GoogleUser, GoogleAuthError> {
    let client = reqwest::Client::builder()
        .timeout(TOKENINFO_TIMEOUT)
        .build()
        .map_err(|e| {
            error!(%e, "failed to build HTTP client");
            GoogleAuthError::RequestFailed
        })?;

    let response = client
        .get("https://oauth2.googleapis.com/tokeninfo")
        .query(&[("id_token", id_token)])
        .send()
        .await
        .map_err(|e| {
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

    if !info.email_is_verified() {
        warn!(email = %info.email, "Google account email is not verified");
        return Err(GoogleAuthError::EmailNotVerified);
    }

    Ok(GoogleUser {
        sub: info.sub,
        email: info.email,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(value: Option<serde_json::Value>) -> TokenInfoResponse {
        TokenInfoResponse {
            sub: "sub".to_string(),
            email: "user@example.com".to_string(),
            aud: "client".to_string(),
            email_verified: value,
        }
    }

    #[test]
    fn accepts_boolean_and_string_verified_flags() {
        assert!(response(Some(serde_json::Value::Bool(true))).email_is_verified());
        assert!(response(Some(serde_json::Value::String("true".to_string()))).email_is_verified());
    }

    #[test]
    fn rejects_unverified_or_missing_flags() {
        assert!(!response(Some(serde_json::Value::Bool(false))).email_is_verified());
        assert!(
            !response(Some(serde_json::Value::String("false".to_string()))).email_is_verified()
        );
        assert!(!response(None).email_is_verified());
    }
}
