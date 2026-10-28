//! Service-to-service authentication for the internal gRPC surface.
//!
//! chat-service is only reachable from the compose network, but that is not an
//! authorization boundary: any container or host process on that network could
//! otherwise write and read every tenant's messages. Each call must carry the
//! shared bearer secret from `GRPC_SHARED_SECRET`.

use tonic::{Request, Status};

pub const AUTH_HEADER: &str = "authorization";

pub fn require_shared_secret(service: &str) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(secret) = std::env::var("GRPC_SHARED_SECRET")
        .ok()
        .filter(|secret| !secret.is_empty())
    {
        return Ok(secret);
    }

    if std::env::var("ALLOW_INSECURE_GRPC").ok().as_deref() == Some("1") {
        tracing::warn!(
            service,
            "GRPC_SHARED_SECRET is unset and ALLOW_INSECURE_GRPC=1: gRPC is unauthenticated"
        );
        return Ok(String::new());
    }

    Err(format!(
        "GRPC_SHARED_SECRET must be set for {service} (or ALLOW_INSECURE_GRPC=1 for local development)"
    )
    .into())
}

pub fn authorize(expected: &str, request: Request<()>) -> Result<Request<()>, Status> {
    if expected.is_empty() {
        return Ok(request);
    }

    let presented = request
        .metadata()
        .get(AUTH_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| Status::unauthenticated("missing service credentials"))?;

    if constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
        Ok(request)
    } else {
        Err(Status::unauthenticated("invalid service credentials"))
    }
}

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
        assert!(authorize("s3cret", request_with(Some("Bearer s3cret"))).is_ok());
    }

    #[test]
    fn rejects_missing_or_wrong_credentials() {
        assert!(authorize("s3cret", request_with(None)).is_err());
        assert!(authorize("s3cret", request_with(Some("Bearer wrong"))).is_err());
        assert!(authorize("s3cret", request_with(Some("s3cret"))).is_err());
    }
}
