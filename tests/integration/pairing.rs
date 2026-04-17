//! Integration tests for pairing invites and folder templates.

use ed25519_dalek::SigningKey;
use murmur_seed::{PairingError, PairingToken};
use murmur_types::DeviceId;

fn new_keypair() -> (DeviceId, SigningKey) {
    let sk = SigningKey::from_bytes(&rand::random());
    let id = DeviceId::from_verifying_key(&sk.verifying_key());
    (id, sk)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Device A issues an invite; device B redeems the URL and receives the
/// correct mnemonic. Mirrors the happy path the desktop/CLI pairing flow
/// rides on.
#[test]
fn test_invite_redeem_end_to_end() {
    let (issuer_id, signing_key) = new_keypair();
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    let token = PairingToken::issue_default(mnemonic, issuer_id, &signing_key);
    let url = token.to_url();
    assert!(url.starts_with("murmur://join?token="));

    // Joiner (fresh device with no network knowledge) redeems the URL
    // purely by parsing the public URL — no network round-trip.
    let parsed = PairingToken::from_url(&url).expect("decode URL");
    let recovered = parsed.redeem(unix_now()).expect("redeem");
    assert_eq!(recovered, mnemonic);

    // Both sides can derive the same NetworkIdentity from the recovered
    // phrase — this is the gate that proves Device B can actually join
    // Device A's network.
    let m_issued = murmur_seed::parse_mnemonic(mnemonic).unwrap();
    let m_recovered = murmur_seed::parse_mnemonic(&recovered).unwrap();
    let id_a = murmur_seed::NetworkIdentity::from_mnemonic(&m_issued, "");
    let id_b = murmur_seed::NetworkIdentity::from_mnemonic(&m_recovered, "");
    assert_eq!(id_a.network_id(), id_b.network_id());
}

/// An invite signed with a different device's key must not verify, even if
/// its `issued_by` matches a legitimate device ID.
#[test]
fn test_invite_signed_by_wrong_key_rejected() {
    let (real_id, _real_sk) = new_keypair();
    let (_fake_id, fake_sk) = new_keypair();

    // Fake issuer tries to impersonate the real device.
    let token = PairingToken::issue_default(
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        real_id,
        &fake_sk,
    );
    let url = token.to_url();
    let parsed = PairingToken::from_url(&url).unwrap();
    assert!(matches!(
        parsed.redeem(unix_now()),
        Err(PairingError::BadSignature)
    ));
}

/// Expired invite is rejected.
#[test]
fn test_invite_expired_rejected() {
    let (id, sk) = new_keypair();
    let token = PairingToken::issue("mnemonic", id, &sk, 1000);
    assert!(matches!(token.redeem(2000), Err(PairingError::Expired)));
}

/// Every built-in folder template produces non-empty, line-separated
/// ignore patterns — guards against a broken cross-crate re-export.
#[test]
fn test_every_template_has_patterns() {
    for slug in murmur_ipc::templates::TEMPLATES {
        let p = murmur_ipc::templates::template_patterns(slug).expect("patterns");
        assert!(p.contains('\n'), "{slug} template should be multi-line");
    }
}
