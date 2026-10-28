//! Redaction helpers.
//!
//! Credentials and bearer tokens must never reach a log sink. These helpers
//! keep enough of a value to correlate events without making it usable.

/// Shows a short prefix and the length, never the secret itself.
///
/// A token like `pxtok_9f3ab12c...` becomes `pxtok_9f3a…(len 30)`.
pub fn token_hint(token: &str) -> String {
    let visible: String = token.chars().take(10).collect();
    format!("{visible}…(len {})", token.chars().count())
}

/// Removes the password from a URL with userinfo, e.g.
/// `redis://:secret@host:6379` becomes `redis://:***@host:6379`.
///
/// Falls back to the original string when there is no userinfo, so callers can
/// log the result unconditionally.
pub fn redact_url_password(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };

    let authority_start = scheme_end + 3;
    let Some(at_offset) = url[authority_start..].find('@') else {
        return url.to_string();
    };
    let at = authority_start + at_offset;

    let userinfo = &url[authority_start..at];

    // Keep the username when present, mask everything from the first colon.
    let redacted_userinfo = match userinfo.find(':') {
        Some(colon) => format!("{}:***", &userinfo[..colon]),
        None => userinfo.to_string(),
    };

    format!(
        "{}{}@{}",
        &url[..authority_start],
        redacted_userinfo,
        &url[at + 1..]
    )
}

/// Constant-time byte comparison.
///
/// Length is compared first (it is not secret for fixed-size keys); the
/// contents are compared without an early return so timing does not reveal how
/// many bytes matched.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
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

    #[test]
    fn token_hint_keeps_a_prefix_but_not_the_secret() {
        let hint = token_hint("pxtok_abcdefghijklmnop");
        assert!(hint.starts_with("pxtok_abcd"));
        assert!(!hint.contains("efghijklmnop"));
        assert!(hint.contains("len 22"));
    }

    #[test]
    fn redacts_password_but_keeps_host() {
        assert_eq!(
            redact_url_password("redis://:s3cret@redis:6379"),
            "redis://:***@redis:6379"
        );
        assert_eq!(
            redact_url_password("postgres://user:pw@db:5432/app"),
            "postgres://user:***@db:5432/app"
        );
    }

    #[test]
    fn leaves_urls_without_userinfo_alone() {
        assert_eq!(
            redact_url_password("redis://127.0.0.1:6379"),
            "redis://127.0.0.1:6379"
        );
        assert_eq!(redact_url_password("not a url"), "not a url");
    }

    #[test]
    fn constant_time_eq_matches_normal_equality() {
        assert!(constant_time_eq(b"peroxo_secret", b"peroxo_secret"));
        assert!(!constant_time_eq(b"peroxo_secret", b"peroxo_secreT"));
        assert!(!constant_time_eq(b"short", b"longer"));
        assert!(constant_time_eq(b"", b""));
    }
}
