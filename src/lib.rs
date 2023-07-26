pub mod channel;
pub mod crypto;
pub mod profile;
pub mod resource;
pub mod text;

use crypto::{KeyPair, PublicKey, Signature};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signed<T: Clone + Serialize> {
    pub key: String,
    pub server: String,
    pub timestamp: u64,
    #[serde(flatten)]
    pub data: T,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub signature: String,
}

impl<T: Clone + Serialize> Signed<T> {
    pub fn new(key_pair: &KeyPair, server: String, timestamp: u64, data: T) -> Option<Self> {
        let Some(public_key) = key_pair.public_key() else {
            return None;
        };

        let mut signed = Signed {
            key: public_key.to_base64(),
            server,
            timestamp,
            data,
            signature: String::new(),
        };

        let Ok(serialized) = serde_json::to_string(&signed) else {
            return None;
        };

        let Some(signature) = key_pair.sign(serialized.as_bytes()) else {
            return None;
        };

        signed.signature = signature.to_base64();

        Some(signed)
    }

    pub fn verify(&self) -> bool {
        let Some(public_key) = PublicKey::from_base64(&self.key) else {
            return false;
        };

        let Some(signature) = Signature::from_base64(&self.signature) else {
            return false;
        };

        let mut signed = self.clone();
        signed.signature = String::new();

        let Ok(serialized) = serde_json::to_string(&signed) else {
            return false;
        };

        public_key.verify(serialized.as_bytes(), &signature)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: '{}'", self.status, self.message)
    }
}

impl std::error::Error for Error {}
