//! QR-based pairing invitations.
//!
//! A [`PairingToken`] is a signed authorization for a new device to join a
//! Murmur network without typing the 12-word mnemonic. It rides inside a
//! `murmur://join?token=…` URL, which is displayed as a QR code. The token
//! carries the network's mnemonic, encrypted with a key derived from a random
//! per-invite nonce, together with an ed25519 signature from an existing
//! device in the network.
//!
//! # Flow
//!
//! Issuer (existing device):
//! 1. Generate random 32-byte `nonce` and record `expires_at_unix` (now + 5 min).
//! 2. Derive symmetric key via HKDF-SHA256 of the nonce.
//! 3. Encrypt the mnemonic phrase using AES-256-GCM.
//! 4. Sign `(nonce || expires_at_unix || issued_by || mnemonic_ciphertext)`
//!    with the device's ed25519 signing key.
//! 5. Bundle into a [`PairingToken`], encode as `murmur://join?token=…`.
//!
//! Joiner:
//! 1. Parse URL, decode token.
//! 2. Verify the signature against `issued_by` (ed25519 public key).
//! 3. Check `expires_at_unix` against the current time.
//! 4. Derive the symmetric key from the nonce and decrypt the mnemonic.
//!
//! # Security notes
//!
//! - The symmetric key is deterministic from the nonce, which rides in the
//!   URL. Anyone who observes the URL (shoulder-surfer, camera, clipboard
//!   logger) can decrypt the mnemonic — the same threat model as reading a
//!   paper-written mnemonic. The value-add is: short expiry, single-use,
//!   and authenticated origin (not a stranger's forged invite).
//! - Single-use is enforced by the issuer keeping a set of used nonces for
//!   the duration of the expiry window.
//! - Forward secrecy against later URL capture is NOT provided. A proper
//!   two-message handshake would require a network round trip, which the
//!   single-QR UX forbids.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use murmur_types::DeviceId;
use rand::TryRng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// URL scheme for pairing invites.
pub const MURMUR_URL_SCHEME: &str = "murmur";

/// URL host (after scheme) for join invites: `murmur://join?token=…`.
pub const JOIN_URL_HOST: &str = "join";

/// Default expiry window for a new invite (5 minutes).
pub const DEFAULT_EXPIRY_SECS: u64 = 5 * 60;

/// HKDF info string for deriving the symmetric encryption key from the nonce.
const HKDF_INFO: &[u8] = b"murmur/pair-invite/v1";

/// Fixed AES-GCM nonce for pair-invite encryption. Safe because the HKDF key
/// is derived from a fresh 32-byte random nonce per invite, so each invite
/// uses a unique (key, nonce) pair.
const AES_NONCE: [u8; 12] = [0u8; 12];

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced when issuing or redeeming a pairing invite.
#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    /// The URL was not a valid `murmur://join?token=…` invite.
    #[error("invalid invite URL")]
    InvalidUrl,

    /// The token bytes failed to decode.
    #[error("failed to decode token: {0}")]
    Decode(String),

    /// The token's signature did not verify against `issued_by`.
    #[error("signature verification failed")]
    BadSignature,

    /// The token has expired.
    #[error("invite has expired")]
    Expired,

    /// Decryption of the mnemonic ciphertext failed (corrupt / tampered).
    #[error("failed to decrypt mnemonic")]
    BadMnemonic,
}

// ---------------------------------------------------------------------------
// PairingToken
// ---------------------------------------------------------------------------

/// A short-lived, signed invite that delivers the network mnemonic to a
/// joining device.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingToken {
    /// 32 random bytes uniquely identifying this invite. Derives the
    /// symmetric encryption key and acts as the single-use identifier.
    pub nonce: [u8; 32],
    /// UNIX timestamp (seconds) at which this invite stops being valid.
    pub expires_at_unix: u64,
    /// Device that issued this invite. Its public key verifies the signature.
    pub issued_by: DeviceId,
    /// AES-256-GCM ciphertext of the mnemonic phrase (as UTF-8 bytes).
    pub mnemonic_ciphertext: Vec<u8>,
    /// Ed25519 signature (64 bytes) over `signing_payload(nonce,
    /// expires_at_unix, issued_by, mnemonic_ciphertext)`.
    ///
    /// Stored as `Vec<u8>` instead of `[u8; 64]` because serde doesn't
    /// derive `Deserialize` for arrays larger than 32 elements.
    pub signature: Vec<u8>,
}

impl std::fmt::Debug for PairingToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairingToken")
            .field("nonce", &"[32 bytes]")
            .field("expires_at_unix", &self.expires_at_unix)
            .field("issued_by", &self.issued_by)
            .field("mnemonic_ciphertext_len", &self.mnemonic_ciphertext.len())
            .field("signature", &"[64 bytes]")
            .finish()
    }
}

/// Build the payload that gets signed (and verified).
fn signing_payload(
    nonce: &[u8; 32],
    expires_at_unix: u64,
    issued_by: &DeviceId,
    mnemonic_ciphertext: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + 8 + 32 + mnemonic_ciphertext.len());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&expires_at_unix.to_be_bytes());
    out.extend_from_slice(issued_by.as_bytes());
    out.extend_from_slice(mnemonic_ciphertext);
    out
}

/// Derive the AES-256-GCM key from the invite nonce.
fn derive_key(nonce: &[u8; 32]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, nonce);
    let mut out = [0u8; 32];
    hk.expand(HKDF_INFO, &mut out)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    out
}

impl PairingToken {
    /// Issue a new invite that delivers `mnemonic` to whoever redeems it.
    ///
    /// `expires_at_unix` must be set to a future UNIX second timestamp.
    /// For the default 5-minute window, see [`DEFAULT_EXPIRY_SECS`].
    pub fn issue(
        mnemonic: &str,
        issuer_id: DeviceId,
        issuer_key: &SigningKey,
        expires_at_unix: u64,
    ) -> Self {
        // Random nonce.
        let mut nonce = [0u8; 32];
        rand::rngs::SysRng
            .try_fill_bytes(&mut nonce)
            .expect("OS RNG should not fail");

        // Encrypt mnemonic.
        let key = derive_key(&nonce);
        let cipher = <aes_gcm::Aes256Gcm as aes_gcm::KeyInit>::new(key.as_slice().into());
        let ciphertext = <aes_gcm::Aes256Gcm as aes_gcm::aead::Aead>::encrypt(
            &cipher,
            (&AES_NONCE).into(),
            mnemonic.as_bytes(),
        )
        .expect("AES-GCM encryption of mnemonic cannot fail");

        // Sign payload.
        let payload = signing_payload(&nonce, expires_at_unix, &issuer_id, &ciphertext);
        let sig: Signature = issuer_key.sign(&payload);

        Self {
            nonce,
            expires_at_unix,
            issued_by: issuer_id,
            mnemonic_ciphertext: ciphertext,
            signature: sig.to_bytes().to_vec(),
        }
    }

    /// Issue with a default 5-minute expiry counted from the current time.
    pub fn issue_default(mnemonic: &str, issuer_id: DeviceId, issuer_key: &SigningKey) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        Self::issue(mnemonic, issuer_id, issuer_key, now + DEFAULT_EXPIRY_SECS)
    }

    /// Verify signature and expiry against `now_unix`, then decrypt and return
    /// the mnemonic phrase.
    ///
    /// Callers should additionally enforce single-use by tracking the token's
    /// [`Self::nonce`] in daemon state.
    pub fn redeem(&self, now_unix: u64) -> Result<String, PairingError> {
        // Signature first — cheap and authenticates origin.
        let vk = VerifyingKey::from_bytes(self.issued_by.as_bytes())
            .map_err(|_| PairingError::BadSignature)?;
        let sig_bytes: [u8; 64] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| PairingError::BadSignature)?;
        let sig = Signature::from_bytes(&sig_bytes);
        let payload = signing_payload(
            &self.nonce,
            self.expires_at_unix,
            &self.issued_by,
            &self.mnemonic_ciphertext,
        );
        vk.verify(&payload, &sig)
            .map_err(|_| PairingError::BadSignature)?;

        if now_unix >= self.expires_at_unix {
            return Err(PairingError::Expired);
        }

        let key = derive_key(&self.nonce);
        let cipher = <aes_gcm::Aes256Gcm as aes_gcm::KeyInit>::new(key.as_slice().into());
        let plaintext = <aes_gcm::Aes256Gcm as aes_gcm::aead::Aead>::decrypt(
            &cipher,
            (&AES_NONCE).into(),
            self.mnemonic_ciphertext.as_slice(),
        )
        .map_err(|_| PairingError::BadMnemonic)?;

        String::from_utf8(plaintext).map_err(|_| PairingError::BadMnemonic)
    }

    /// Encode as a `murmur://join?token=…` URL.
    pub fn to_url(&self) -> String {
        let bytes = postcard::to_allocvec(self).expect("PairingToken serializes");
        let encoded = data_encoding::BASE64URL_NOPAD.encode(&bytes);
        format!("{MURMUR_URL_SCHEME}://{JOIN_URL_HOST}?token={encoded}")
    }

    /// Parse a `murmur://join?token=…` URL into a token.
    ///
    /// Performs no signature / expiry validation; call [`Self::redeem`] for
    /// those.
    pub fn from_url(url: &str) -> Result<Self, PairingError> {
        let prefix = format!("{MURMUR_URL_SCHEME}://{JOIN_URL_HOST}?token=");
        let rest = url.strip_prefix(&prefix).ok_or(PairingError::InvalidUrl)?;
        // Allow trailing query params or fragments: we take up to first '&' or '#'.
        let encoded = rest.split(['&', '#']).next().unwrap_or(rest);
        let bytes = data_encoding::BASE64URL_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|e| PairingError::Decode(e.to_string()))?;
        postcard::from_bytes(&bytes).map_err(|e| PairingError::Decode(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WordCount, generate_mnemonic};

    fn issuer() -> (DeviceId, SigningKey) {
        let kp = crate::DeviceKeyPair::generate();
        (kp.device_id(), kp.signing_key().clone())
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn test_issue_redeem_roundtrip() {
        let (id, key) = issuer();
        let m = generate_mnemonic(WordCount::Twelve).to_string();
        let t = PairingToken::issue(&m, id, &key, now() + 60);
        let decoded = t.redeem(now()).unwrap();
        assert_eq!(decoded, m);
    }

    #[test]
    fn test_signature_verifies_with_issuer_key() {
        let (id, key) = issuer();
        let t = PairingToken::issue("test mnemonic phrase", id, &key, now() + 60);
        // Corrupt a byte in ciphertext — signature should no longer verify.
        let mut tampered = t.clone();
        tampered.mnemonic_ciphertext[0] ^= 0x01;
        match tampered.redeem(now()) {
            Err(PairingError::BadSignature) => {}
            other => panic!("expected BadSignature, got {other:?}"),
        }
    }

    #[test]
    fn test_wrong_issuer_rejected() {
        let (_id_a, key_a) = issuer();
        let (id_b, _) = issuer();
        // Token claims to be from B but is signed by A.
        let mut raw = PairingToken::issue("mnemonic here", id_b, &key_a, now() + 60);
        // Rebuild signature so that issued_by says B but key was A — set sig
        // using A's key (as `issue` does) then change issued_by to B's id.
        raw.issued_by = id_b;
        match raw.redeem(now()) {
            Err(PairingError::BadSignature) => {}
            other => panic!("expected BadSignature, got {other:?}"),
        }
    }

    #[test]
    fn test_expired_token_rejected() {
        let (id, key) = issuer();
        let t = PairingToken::issue("abc", id, &key, 1000);
        match t.redeem(2000) {
            Err(PairingError::Expired) => {}
            other => panic!("expected Expired, got {other:?}"),
        }
    }

    #[test]
    fn test_url_roundtrip() {
        let (id, key) = issuer();
        let m = generate_mnemonic(WordCount::TwentyFour).to_string();
        let t = PairingToken::issue(&m, id, &key, now() + 60);
        let url = t.to_url();
        assert!(url.starts_with("murmur://join?token="));
        let parsed = PairingToken::from_url(&url).unwrap();
        assert_eq!(parsed, t);
        assert_eq!(parsed.redeem(now()).unwrap(), m);
    }

    #[test]
    fn test_from_url_rejects_wrong_scheme() {
        let err = PairingToken::from_url("https://example.com/?token=abc").unwrap_err();
        assert!(matches!(err, PairingError::InvalidUrl));
    }

    #[test]
    fn test_from_url_ignores_trailing_fragment() {
        let (id, key) = issuer();
        let t = PairingToken::issue("abc", id, &key, now() + 60);
        let url = format!("{}#frag", t.to_url());
        let parsed = PairingToken::from_url(&url).unwrap();
        assert_eq!(parsed, t);
    }

    #[test]
    fn test_issue_default_expiry_window() {
        let (id, key) = issuer();
        let t = PairingToken::issue_default("mnem", id, &key);
        let n = now();
        assert!(t.expires_at_unix > n);
        assert!(t.expires_at_unix <= n + DEFAULT_EXPIRY_SECS + 2);
    }

    #[test]
    fn test_nonce_unique_per_issue() {
        let (id, key) = issuer();
        let t1 = PairingToken::issue("a", id, &key, now() + 60);
        let t2 = PairingToken::issue("a", id, &key, now() + 60);
        assert_ne!(t1.nonce, t2.nonce);
        assert_ne!(t1.mnemonic_ciphertext, t2.mnemonic_ciphertext);
    }

    #[test]
    fn test_tampered_ciphertext_fails_sig() {
        let (id, key) = issuer();
        let mut t = PairingToken::issue("m", id, &key, now() + 60);
        // Flip a bit in the ciphertext — sig covers ciphertext, so it fails.
        t.mnemonic_ciphertext.push(0x42);
        assert!(matches!(t.redeem(now()), Err(PairingError::BadSignature)));
    }
}
