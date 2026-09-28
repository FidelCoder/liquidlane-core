use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use liquidlane_core::marketplace::joyid::{Proof, verify_signature};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::json;
use sha2::{Digest, Sha256};

fn webauthn(challenge: &str, origin: &str, domain: &str, flags: u8) -> Proof {
    let key = SigningKey::from_slice(&[7u8; 32]).unwrap();
    let client=json!({"type":"webauthn.get","challenge":URL_SAFE_NO_PAD.encode(challenge),"origin":origin,"crossOrigin":false}).to_string();
    let mut auth = Sha256::digest(domain).to_vec();
    auth.extend_from_slice(&[flags, 0, 0, 0, 1]);
    let mut signed = auth.clone();
    signed.extend_from_slice(&Sha256::digest(client.as_bytes()));
    let sig: Signature = key.sign(&signed);
    auth.extend_from_slice(client.as_bytes());
    Proof {
        scheme: "joyid".into(),
        address: String::new(),
        pubkey: hex::encode(&key.verifying_key().to_encoded_point(false).as_bytes()[1..]),
        signature: URL_SAFE_NO_PAD.encode(sig.to_der()),
        message: URL_SAFE_NO_PAD.encode(auth),
        challenge: challenge.into(),
        key_type: "main_key".into(),
        alg: -7,
    }
}

#[test]
fn verifies_real_p256_authenticator_signature_and_binds_origin_rp_challenge() {
    let origin = "https://testnet.joyid.dev";
    let p = webauthn("unique challenge", origin, "joyid.dev", 1);
    assert!(verify_signature(&p, "unique challenge", origin).is_ok());
    assert!(verify_signature(&p, "different challenge", origin).is_err());
    assert!(verify_signature(&p, "unique challenge", "https://evil.example").is_err());
    let mut altered = p.clone();
    altered.signature = URL_SAFE_NO_PAD.encode([0; 64]);
    assert!(verify_signature(&altered, "unique challenge", origin).is_err());
    let rp = webauthn("unique challenge", origin, "evil.example", 1);
    assert!(verify_signature(&rp, "unique challenge", origin).is_err());
    let absent = webauthn("unique challenge", origin, "joyid.dev", 0);
    assert!(verify_signature(&absent, "unique challenge", origin).is_err());
}
