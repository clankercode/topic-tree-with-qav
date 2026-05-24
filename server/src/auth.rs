//! Identifiers, secret tokens, and password-hash helpers for room admin auth.
//!
//! Tokens are NEVER persisted in raw form: the database stores only the
//! argon2id hash. Verification is done once at `Hello` (see `protocol.md`)
//! and the resulting role is cached for the connection's lifetime.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::{rngs::OsRng as RandOsRng, RngCore};

/// 12-character URL-safe identifier for a room. Drawn from RFC-4648 base32
/// (`A-Z2-7`), which is case-insensitive-safe and contains no ambiguous
/// path characters. Carries ~60 bits of entropy.
pub const ROOM_ID_LEN: usize = 12;

/// 32 random bytes = 256 bits of entropy, encoded as URL-safe base64
/// without padding (43 ASCII chars). Suitable for a bearer credential
/// embedded in URLs and held client-side in IndexedDB.
pub const ADMIN_TOKEN_BYTES: usize = 32;

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Generate a fresh 12-character base32 room id from OS randomness.
pub fn new_room_id() -> String {
    let mut rng = RandOsRng;
    let mut out = String::with_capacity(ROOM_ID_LEN);
    for _ in 0..ROOM_ID_LEN {
        // 5-bit index per character; biased? No — we mask 5 bits of a
        // freshly drawn u32 each loop, which is uniform over [0, 32).
        let idx = (rng.next_u32() & 0x1f) as usize;
        out.push(BASE32_ALPHABET[idx] as char);
    }
    out
}

/// Generate a fresh admin token: 32 random bytes encoded as URL-safe
/// base64 without padding.
pub fn new_admin_token() -> String {
    let mut bytes = [0u8; ADMIN_TOKEN_BYTES];
    RandOsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Returns `true` iff `id` is a syntactically valid room id (12 chars from
/// the base32 alphabet). Does not check existence in the database.
pub fn is_valid_room_id(id: &str) -> bool {
    id.len() == ROOM_ID_LEN && id.bytes().all(|b| matches!(b, b'A'..=b'Z' | b'2'..=b'7'))
}

/// Guest id must be a non-empty string of printable ASCII or visible Unicode
/// chars (no control chars / whitespace), up to 64 bytes. We don't require
/// UUIDv4 specifically because the client mints these.
pub fn is_valid_guest_id(id: &str) -> bool {
    let len = id.len();
    if len == 0 || len > 64 {
        return false;
    }
    id.chars().all(|c| !c.is_whitespace() && !c.is_control())
}

/// Display name: 1..=64 characters, trimmed. Disallow control chars but allow
/// emoji / unicode letters.
pub fn is_valid_display_name(name: &str) -> bool {
    let trimmed = name.trim();
    let len = trimmed.chars().count();
    if len == 0 || len > 64 {
        return false;
    }
    trimmed.chars().all(|c| !c.is_control())
}

/// Hash an admin token with argon2id using a fresh random salt. CPU-bound:
/// callers in async contexts must wrap in `spawn_blocking`.
pub fn hash_admin_token(token: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    let hash = argon
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| AuthError::Hash(e.to_string()))?;
    Ok(hash.to_string())
}

/// Verify a token against a previously stored PHC string. CPU-bound:
/// callers in async contexts must wrap in `spawn_blocking`.
///
/// Returns `Ok(true)` on a match, `Ok(false)` on a non-match, and an
/// `Err` only when the stored hash itself is malformed (a server bug or
/// a corrupted row, not a wrong password).
pub fn verify_admin_token(token: &str, phc: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(phc).map_err(|e| AuthError::Hash(e.to_string()))?;
    match Argon2::default().verify_password(token.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(e) => Err(AuthError::Hash(e.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("argon2 error: {0}")]
    Hash(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn room_id_is_twelve_chars_from_alphabet() {
        for _ in 0..50 {
            let id = new_room_id();
            assert_eq!(id.len(), ROOM_ID_LEN);
            assert!(is_valid_room_id(&id), "rejected {id}");
        }
    }

    #[test]
    fn room_ids_are_unique_with_high_probability() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(new_room_id()), "collision in 1000 draws");
        }
    }

    #[test]
    fn rejects_lowercase_and_non_alphabet() {
        assert!(!is_valid_room_id("abcdefghijkl"));
        assert!(!is_valid_room_id("ABCDEFGH1234")); // '1' not in alphabet
        assert!(!is_valid_room_id("ABCDEFGHIJK")); // too short
        assert!(!is_valid_room_id("ABCDEFGHIJKLM")); // too long
        assert!(!is_valid_room_id(""));
    }

    #[test]
    fn admin_token_is_url_safe_base64_no_pad() {
        let t = new_admin_token();
        // 32 bytes → 43 chars unpadded.
        assert_eq!(t.len(), 43);
        assert!(t
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        // Round-trip decodes to exactly 32 bytes.
        let raw = URL_SAFE_NO_PAD.decode(&t).expect("decode");
        assert_eq!(raw.len(), ADMIN_TOKEN_BYTES);
    }

    #[test]
    fn admin_tokens_are_unique() {
        let mut seen = HashSet::new();
        for _ in 0..100 {
            assert!(seen.insert(new_admin_token()));
        }
    }

    #[test]
    fn hash_then_verify_succeeds_on_match() {
        let token = new_admin_token();
        let hash = hash_admin_token(&token).expect("hash");
        // Hash is a PHC string starting with `$argon2id$`.
        assert!(hash.starts_with("$argon2id$"));
        // The hash never contains the raw token.
        assert!(!hash.contains(&token));
        assert!(verify_admin_token(&token, &hash).expect("verify ok"));
    }

    #[test]
    fn verify_rejects_wrong_token() {
        let token = new_admin_token();
        let other = new_admin_token();
        let hash = hash_admin_token(&token).unwrap();
        assert!(!verify_admin_token(&other, &hash).unwrap());
    }

    #[test]
    fn verify_errors_on_malformed_phc() {
        let err = verify_admin_token("anything", "not-a-phc-string").unwrap_err();
        assert!(matches!(err, AuthError::Hash(_)));
    }

    #[test]
    fn hashes_use_distinct_salts() {
        let token = new_admin_token();
        let h1 = hash_admin_token(&token).unwrap();
        let h2 = hash_admin_token(&token).unwrap();
        assert_ne!(
            h1, h2,
            "two hashings of same token must differ (random salt)"
        );
    }
}
