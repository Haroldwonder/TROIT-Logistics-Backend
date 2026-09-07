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

impl SorobanSigner {
    pub fn from_secret_key(secret_strkey: &str) -> Result<Self, BlockchainError> {
        let secret = secret_strkey.trim();
        if secret.is_empty() {
            return Err(BlockchainError::AuthError(
                "Secret key string is empty".to_string(),
            ));
        }

        let strkey = Strkey::from_string(secret).map_err(|e| {
            BlockchainError::AuthError(format!("Failed to parse secret strkey: {}", e))
        })?;

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
        let public_key_str =
            Strkey::PublicKeyEd25519(ed25519::PublicKey(raw_public_bytes)).to_string();

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
