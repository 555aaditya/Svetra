//! Reading the `role` of a JWT without checking its signature or touching the network
//! (docs/doctor.md, The anon key is not a leak). Doctor judges a key by what it decodes to.

/// The decoded value of a base64url string, or `None` if it isn't valid base64url.
/// Padding is optional.
pub fn base64url(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // One leftover character can't encode a byte.
    if s.len() % 4 == 1 { None } else { Some(out) }
}

/// What a JWT-shaped token says about itself.
#[derive(Debug, PartialEq, Eq)]
pub enum Role {
    /// The `role` claim is `service_role`: it skips row-level security.
    Service,
    /// The `role` claim is `anon`: public by design.
    Anon,
    /// A JWT with no `role`, or any other role (`authenticated`, a user's session, ...).
    Other,
}

/// `Some(role)` when `token` is three base64url parts whose middle part is a JSON object, and
/// `None` when it isn't a JWT at all. The signature is never checked.
pub fn role(token: &str) -> Option<Role> {
    let mut parts = token.split('.');
    let (header, payload, signature) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || header.is_empty() || payload.is_empty() || signature.is_empty() {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_slice(&base64url(payload)?).ok()?;
    let claims = payload.as_object()?;
    Some(match claims.get("role").and_then(|r| r.as_str()) {
        Some("service_role") => Role::Service,
        Some("anon") => Role::Anon,
        _ => Role::Other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..=chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            }
        }
        out
    }

    fn jwt(payload: &str) -> String {
        format!(
            "{}.{}.sig",
            encode(b"{\"alg\":\"HS256\"}"),
            encode(payload.as_bytes())
        )
    }

    #[test]
    fn decodes_base64url_with_and_without_padding() {
        assert_eq!(base64url("aGk").as_deref(), Some(&b"hi"[..]));
        assert_eq!(base64url("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(base64url("a").as_deref(), None);
        assert_eq!(base64url("a+b/").as_deref(), None);
    }

    #[test]
    fn reads_the_role_claim() {
        assert_eq!(
            role(&jwt(r#"{"role":"service_role"}"#)),
            Some(Role::Service)
        );
        assert_eq!(role(&jwt(r#"{"role":"anon"}"#)), Some(Role::Anon));
        assert_eq!(role(&jwt(r#"{"role":"authenticated"}"#)), Some(Role::Other));
        assert_eq!(role(&jwt(r#"{"sub":"1"}"#)), Some(Role::Other));
    }

    #[test]
    fn rejects_what_is_not_a_jwt() {
        assert_eq!(role("eyJ.not.json"), None);
        assert_eq!(role("a.b"), None);
        assert_eq!(role(&jwt("[1]")), None);
        assert_eq!(role(&jwt("not json")), None);
    }
}
