use super::crypto;
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use rsa::{BigUint, RsaPublicKey, pkcs1v15};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proof {
    pub scheme: String,
    pub address: String,
    pub pubkey: String,
    pub signature: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub challenge: String,
    #[serde(default, rename = "keyType")]
    pub key_type: String,
    #[serde(default)]
    pub alg: i32,
}

pub async fn verify(client: &reqwest::Client, proof: &Proof, expected: &str) -> Result<()> {
    crypto::address_script(&proof.address)?;
    if proof.scheme == "ckb_secp256k1" {
        ensure!(
            crypto::same_address(&crypto::native_address(&proof.pubkey)?, &proof.address)?,
            "signature is not bound to the claimed wallet"
        );
        return crypto::verify(&proof.pubkey, expected, &proof.signature);
    }
    ensure!(proof.scheme == "joyid", "unsupported wallet proof scheme");
    verify_signature(proof, expected, "https://testnet.joyid.dev")?;
    let result: serde_json::Value = client
        .get(format!(
            "{}/credentials/{}",
            "https://api.testnet.joyid.dev/api/v1", proof.address
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let credentials = result["credentials"]
        .as_array()
        .context("JoyID credential lookup unavailable")?;
    let valid = credentials.iter().any(|credential| {
        let Some(key) = credential["public_key"].as_str() else {
            return false;
        };
        let key = if proof.alg == -7 && !proof.key_type.ends_with("session_key") {
            key.get(2..).unwrap_or("")
        } else {
            key
        };
        credential["ckb_address"].as_str() == Some(proof.address.as_str())
            && key.eq_ignore_ascii_case(proof.pubkey.trim_start_matches("0x"))
    });
    ensure!(
        valid,
        "JoyID signing credential is not registered to this address"
    );
    Ok(())
}

pub fn verify_signature(proof: &Proof, expected: &str, origin: &str) -> Result<()> {
    ensure!(proof.challenge == expected, "wallet challenge mismatch");
    let message = URL_SAFE_NO_PAD.decode(proof.message.trim_end_matches('='))?;
    let signature = URL_SAFE_NO_PAD.decode(proof.signature.trim_end_matches('='))?;
    let public_key = crypto::bytes(&proof.pubkey)?;
    if matches!(
        proof.key_type.as_str(),
        "main_session_key" | "sub_session_key"
    ) {
        ensure!(
            message == expected.as_bytes(),
            "session key challenge mismatch"
        );
        return rsa_verify(&public_key, &signature, &message);
    }
    ensure!(
        matches!(proof.key_type.as_str(), "main_key" | "sub_key"),
        "unsupported JoyID credential type"
    );
    ensure!(message.len() > 37, "incomplete WebAuthn authenticator data");
    let data: serde_json::Value = serde_json::from_slice(&message[37..])?;
    ensure!(
        data["type"] == "webauthn.get" && data["origin"] == origin,
        "unexpected WebAuthn origin or ceremony"
    );
    ensure!(
        data["crossOrigin"] != true,
        "cross-origin WebAuthn proof is not allowed"
    );
    ensure!(
        data["challenge"] == URL_SAFE_NO_PAD.encode(expected.as_bytes()),
        "WebAuthn challenge mismatch"
    );
    ensure!(message[32] & 1 == 1, "WebAuthn user presence required");
    let host = reqwest::Url::parse(origin)?
        .host_str()
        .context("invalid JoyID origin")?
        .to_owned();
    let domain = host.strip_prefix("testnet.").unwrap_or(&host);
    ensure!(
        message[..32] == Sha256::digest(host.as_bytes())[..]
            || message[..32] == Sha256::digest(domain.as_bytes())[..],
        "WebAuthn RP identity mismatch"
    );
    let mut signed = message[..37].to_vec();
    signed.extend_from_slice(&Sha256::digest(&message[37..]));
    match proof.alg {
        -7 => {
            ensure!(public_key.len() == 64, "invalid P-256 key");
            let mut key = vec![4];
            key.extend_from_slice(&public_key);
            VerifyingKey::from_sec1_bytes(&key)?
                .verify(&signed, &Signature::from_der(&signature)?)?;
        }
        -257 => rsa_verify(&public_key, &signature, &signed)?,
        _ => anyhow::bail!("unsupported JoyID signature algorithm"),
    }
    Ok(())
}

fn rsa_verify(key: &[u8], signature: &[u8], message: &[u8]) -> Result<()> {
    ensure!(
        key.len() >= 260 && key.len() <= 1028,
        "invalid RSA credential"
    );
    let key = RsaPublicKey::new(
        BigUint::from_bytes_le(&key[4..]),
        BigUint::from_bytes_le(&key[..3]),
    )?;
    pkcs1v15::VerifyingKey::<Sha256>::new(key)
        .verify(message, &pkcs1v15::Signature::try_from(signature)?)?;
    Ok(())
}
