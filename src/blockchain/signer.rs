#![allow(dead_code)]

use super::errors::BlockchainError;
use ed25519_dalek::{Signer as DalekSigner, SigningKey};
use std::fmt;
use stellar_strkey::{ed25519, Strkey};

#[derive(Clone)]
pub struct SorobanSigner {
    signing_key: SigningKey,
    public_key_str: String,
    raw_public_bytes: [u8; 32],
}

impl fmt::Debug for SorobanSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SorobanSigner")
            .field("public_key", &self.public_key_str)
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

/// Parses an Ed25519 secret seed strkey and derives the corresponding signing key,
/// public key strkey, and raw public key bytes. Shared by `SorobanSigner::from_secret_key`
/// and `derive_public_key_from_secret` so there is a single derivation path.
fn derive_keypair_from_secret(
    secret_strkey: &str,
) -> Result<(SigningKey, String, [u8; 32]), BlockchainError> {
    let secret = secret_strkey.trim();
    if secret.is_empty() {
        return Err(BlockchainError::AuthError(
            "Secret key string is empty".to_string(),
        ));
    }

    let strkey = Strkey::from_string(secret)
        .map_err(|e| BlockchainError::AuthError(format!("Failed to parse secret strkey: {}", e)))?;

    let seed_bytes = match strkey {
        Strkey::PrivateKeyEd25519(sk) => sk.0,
        _ => {
            return Err(BlockchainError::AuthError(
                "Strkey is not an Ed25519 private secret key".to_string(),
            ))
        }
    };

    let signing_key = SigningKey::from_bytes(&seed_bytes);
    let verifying_key = signing_key.verifying_key();
    let raw_public_bytes = verifying_key.to_bytes();
    let public_key_str = Strkey::PublicKeyEd25519(ed25519::PublicKey(raw_public_bytes)).to_string();

    Ok((signing_key, public_key_str, raw_public_bytes))
}

/// Derives the Ed25519 public key strkey (starts with "G") corresponding to a given
/// secret seed strkey (starts with "S"). Use this whenever a public key is needed
/// from a configured secret key, instead of using the secret key string directly.
pub fn derive_public_key_from_secret(secret_strkey: &str) -> Result<String, BlockchainError> {
    let (_, public_key_str, _) = derive_keypair_from_secret(secret_strkey)?;
    Ok(public_key_str)
}

impl SorobanSigner {
    pub fn from_secret_key(secret_strkey: &str) -> Result<Self, BlockchainError> {
        let (signing_key, public_key_str, raw_public_bytes) =
            derive_keypair_from_secret(secret_strkey)?;

        Ok(Self {
            signing_key,
            public_key_str,
            raw_public_bytes,
        })
    }

    pub fn public_key(&self) -> &str {
        &self.public_key_str
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.raw_public_bytes
    }

    pub fn sign_payload(&self, payload: &[u8]) -> [u8; 64] {
        let signature = self.signing_key.sign(payload);
        signature.to_bytes()
    }
}
