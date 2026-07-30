// a3sql — Ed25519 signature verification for SQF query authentication

//! Optional auth layer: verifies Ed25519-signed SQF payloads before dispatch.
//!
//! When the `auth` Cargo feature is enabled and configured in `a3sql.toml`,
//! every query must carry a `SIGNED <hex_sig> <query>` prefix. The signature
//! covers the raw payload (everything after the hex signature).
//!
//! When the feature is disabled all queries pass through unchanged.

#[cfg(feature = "auth")]
use ed25519_dalek::{Signature, VerifyingKey};

// ── Public API ────────────────────────────────────────────────────────────

/// Parse a `SIGNED <hex_sig> <payload>` input.
///
/// Returns `Some((signature_hex, payload))` when the prefix matches.
/// Returns `None` for unsigned queries.
#[allow(dead_code, reason = "phased auth implementation")]
pub(crate) fn parse_signed_input(input: &str) -> Option<(&str, &str)> {
    let input = input.trim();
    let rest = input.strip_prefix("SIGNED ")?;
    // Signature ends at the next whitespace or at the end of input
    let sig_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let sig_hex = &rest[..sig_end];
    let payload = rest[sig_end..].trim();
    Some((sig_hex, payload))
}

/// Decode a hex-encoded Ed25519 public key into 32 raw bytes.
#[allow(dead_code, reason = "phased auth implementation")]
pub(crate) fn hex_to_pubkey(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

/// Verify an Ed25519 signature.
///
/// Feature‑gated: when `auth` is disabled this is a no‑op returning `true`.
#[cfg(feature = "auth")]
#[allow(dead_code, reason = "phased auth implementation")]
pub(crate) fn verify_signature(public_key: &[u8; 32], payload: &str, signature_hex: &str) -> bool {
    let sig_bytes = match decode_hex(signature_hex) {
        Some(b) if b.len() == 64 => b,
        _ => return false,
    };
    let signature = match Signature::from_slice(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let verifying_key = match VerifyingKey::from_bytes(public_key) {
        Ok(k) => k,
        Err(_) => return false,
    };
    verifying_key.verify_strict(payload.as_bytes(), &signature).is_ok()
}

/// No‑op when auth feature is disabled — always returns `true`.
#[cfg(not(feature = "auth"))]
#[allow(dead_code, reason = "phased auth implementation")]
pub(crate) fn verify_signature(_public_key: &[u8; 32], _payload: &str, _signature_hex: &str) -> bool {
    true
}

// ── Internal helpers ─────────────────────────────────────────────────────

/// Decode a hex string into `Vec<u8>`. Returns `None` on invalid input.
#[allow(dead_code, reason = "phased auth implementation")]
#[allow(clippy::manual_is_multiple_of)]
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || s.is_empty() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_signed_input ──────────────────────────────────────────────

    #[test]
    fn parse_signed_input_valid() {
        let (sig, payload) = parse_signed_input("SIGNED abc123 SELECT 1").unwrap();
        assert_eq!(sig, "abc123");
        assert_eq!(payload, "SELECT 1");
    }

    #[test]
    fn parse_signed_input_no_sig() {
        assert!(parse_signed_input("SELECT 1").is_none());
    }

    #[test]
    fn parse_signed_input_empty() {
        assert!(parse_signed_input("").is_none());
    }

    #[test]
    fn parse_signed_input_only_keyword() {
        assert!(parse_signed_input("SIGNED").is_none());
    }

    #[test]
    fn parse_signed_input_only_keyword_and_sig() {
        let (sig, payload) = parse_signed_input("SIGNED abcd").unwrap();
        assert_eq!(sig, "abcd");
        assert_eq!(payload, "");
    }

    #[test]
    fn parse_signed_input_case_sensitive() {
        assert!(parse_signed_input("signed abc SELECT 1").is_none());
    }

    #[test]
    fn parse_signed_input_leading_whitespace() {
        let (sig, payload) = parse_signed_input("  SIGNED deadbeef SELECT 1").unwrap();
        assert_eq!(sig, "deadbeef");
        assert_eq!(payload, "SELECT 1");
    }

    // ── hex_to_pubkey ───────────────────────────────────────────────────

    #[test]
    fn hex_to_pubkey_valid() {
        let hex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let key = hex_to_pubkey(hex);
        assert!(key.is_some());
        assert_eq!(key.unwrap()[0], 0x01);
    }

    #[test]
    fn hex_to_pubkey_short() {
        assert!(hex_to_pubkey("abcd").is_none());
    }

    #[test]
    fn hex_to_pubkey_invalid_hex() {
        assert!(hex_to_pubkey("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    }

    #[test]
    fn hex_to_pubkey_empty() {
        assert!(hex_to_pubkey("").is_none());
    }

    // ── decode_hex ──────────────────────────────────────────────────────

    #[test]
    fn decode_hex_valid() {
        assert_eq!(decode_hex("deadbeef"), Some(vec![0xde, 0xad, 0xbe, 0xef]));
    }

    #[test]
    fn decode_hex_odd_length() {
        assert!(decode_hex("abc").is_none());
    }

    #[test]
    fn decode_hex_invalid_char() {
        assert!(decode_hex("xyz").is_none());
    }

    // ── verify_signature — auth enabled ─────────────────────────────────

    #[cfg(feature = "auth")]
    #[test]
    fn sign_verify_roundtrip() {
        use ed25519_dalek::{Signer, SigningKey};

        let seed = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let pubkey = verifying_key.to_bytes();

        let payload = "SELECT * FROM players WHERE score > 1000";
        let signature = signing_key.sign(payload.as_bytes());
        let sig_hex = signature
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        assert!(verify_signature(&pubkey, payload, &sig_hex));
    }

    #[cfg(feature = "auth")]
    #[test]
    fn reject_wrong_signature() {
        use ed25519_dalek::{Signer, SigningKey};

        let seed = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let pubkey = verifying_key.to_bytes();

        // Sign payload A, verify against payload B
        let signature = signing_key.sign(b"SELECT 1");
        let sig_hex = signature
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        assert!(!verify_signature(&pubkey, "SELECT 2", &sig_hex));
    }

    #[cfg(feature = "auth")]
    #[test]
    fn reject_tampered_payload() {
        use ed25519_dalek::{Signer, SigningKey};

        let seed = [99u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        let pubkey = verifying_key.to_bytes();

        let payload = "INSERT INTO users VALUES (1, 'alice')";
        let signature = signing_key.sign(payload.as_bytes());
        let sig_hex = signature
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        assert!(!verify_signature(
            &pubkey,
            "INSERT INTO users VALUES (1, 'eve')",
            &sig_hex
        ));
    }

    #[cfg(feature = "auth")]
    #[test]
    fn reject_invalid_hex_sig() {
        let pubkey = [0u8; 32];
        assert!(!verify_signature(&pubkey, "SELECT 1", "nothex"));
    }

    #[cfg(feature = "auth")]
    #[test]
    fn reject_wrong_key() {
        use ed25519_dalek::{Signer, SigningKey};

        let seed_a = [1u8; 32];
        let seed_b = [2u8; 32];
        let signing_key = SigningKey::from_bytes(&seed_a);
        let wrong_pubkey = SigningKey::from_bytes(&seed_b).verifying_key().to_bytes();

        let payload = "SELECT 1";
        let signature = signing_key.sign(payload.as_bytes());
        let sig_hex = signature
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        // Wrong public key → verify fails
        assert!(!verify_signature(&wrong_pubkey, payload, &sig_hex));
    }

    // ── verify_signature — auth disabled ────────────────────────────────

    #[cfg(not(feature = "auth"))]
    #[test]
    fn noop_when_auth_disabled() {
        // When auth feature is off, verify always returns true
        assert!(verify_signature(&[0u8; 32], "anything", "garbage"));
    }
}
