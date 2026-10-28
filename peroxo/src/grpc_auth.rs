//! Service-to-service authentication for internal gRPC calls.
//!
//! The gateway, auth-service, and chat-service sit on a private network, but
//! "private network" is not an authorization boundary: anything that can reach
//! the port can read or write any tenant's messages. Every business RPC now
//! carries a bearer secret that the server verifies before dispatch.
//!
//! Set `GRPC_SHARED_SECRET` in every service. Local development that has not
//! configured one can opt out explicitly with `ALLOW_INSECURE_GRPC=1`; the
//! process logs a warning and runs without the check.

use tonic::metadata::MetadataValue;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::Channel;
use tonic::{Request, Status};

pub const AUTH_HEADER: &str = "authorization";

/// Returns the configured secret, if any.
pub fn shared_secret() -> Option<String> {
    std::env::var("GRPC_SHARED_SECRET")
        .ok()
        .filter(|secret| !secret.is_empty())
}

/// Fails closed unless a secret is configured or insecure mode is explicit.
pub fn require_shared_secret(service: &str) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(secret) = shared_secret() {
        return Ok(secret);
    }

    if std::env::var("ALLOW_INSECURE_GRPC").ok().as_deref() == Some("1") {
        tracing::warn!(
            service,
            "GRPC_SHARED_SECRET is unset and ALLOW_INSECURE_GRPC=1: internal gRPC is unauthenticated"
        );
        return Ok(String::new());
    }

    Err(format!(
        "GRPC_SHARED_SECRET must be set for {service} (or ALLOW_INSECURE_GRPC=1 for local development)"
    )
    .into())
}

/// Verifies the bearer secret carried in request metadata.
///
/// This is the server-side half of the scheme; the gateway only acts as a
/// client today, so it is compiled for tests to keep the rule documented and
/// verified in one place.
#[cfg(test)]
pub fn authorize(expected: &str, request: &Request<()>) -> Result<(), Status> {
    if expected.is_empty() {
        return Ok(());
    }

    let header = request
        .metadata()
        .get(AUTH_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("missing service credentials"))?;

    let presented = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| Status::unauthenticated("invalid service credentials"))?;

    // Constant-time comparison: the secret is fixed-size and must not leak
    // through timing.
    if constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
        Ok(())
    } else {
        Err(Status::unauthenticated("invalid service credentials"))
    }
}

#[cfg(test)]
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }

    difference == 0
}

/// Attaches the bearer secret to every outbound request.
#[derive(Clone)]
pub struct ServiceAuth {
    header: Option<MetadataValue<tonic::metadata::Ascii>>,
}

impl ServiceAuth {
    pub fn new(secret: &str) -> Self {
        Self {
            header: if secret.is_empty() {
                None
            } else {
                Some(
                    format!("Bearer {secret}")
                        .parse()
                        .expect("GRPC_SHARED_SECRET must be a valid ASCII metadata value"),
                )
            },
        }
    }
}

impl tonic::service::Interceptor for ServiceAuth {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        if let Some(header) = &self.header {
            request.metadata_mut().insert(AUTH_HEADER, header.clone());
        }
        Ok(request)
    }
}

/// Client channel wrapper that signs every request.
pub type AuthedChannel = InterceptedService<Channel, ServiceAuth>;

pub type AuthClient = crate::auth_service_client::AuthServiceClient<AuthedChannel>;
pub type ChatClient = crate::chat_service_client::ChatServiceClient<AuthedChannel>;

#[cfg(test)]
mod tests {
    use super::*;

    fn request_with(value: Option<&str>) -> Request<()> {
        let mut request = Request::new(());
        if let Some(value) = value {
            request
                .metadata_mut()
                .insert(AUTH_HEADER, value.parse().unwrap());
        }
        request
    }

    #[test]
    fn accepts_the_expected_secret() {
        assert!(authorize("s3cret", &request_with(Some("Bearer s3cret"))).is_ok());
    }

    #[test]
    fn rejects_missing_or_wrong_credentials() {
        assert!(authorize("s3cret", &request_with(None)).is_err());
        assert!(authorize("s3cret", &request_with(Some("Bearer wrong"))).is_err());
        assert!(authorize("s3cret", &request_with(Some("s3cret"))).is_err());
    }

    #[test]
    fn insecure_mode_skips_the_check() {
        assert!(authorize("", &request_with(None)).is_ok());
    }
}
