use base64::{prelude::BASE64_STANDARD, Engine};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, UnparsedPublicKey, ED25519},
};

/// Ed25519 Constants
pub const ED25519_SIGNATURE_LEN: usize = 64;
pub const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// Ed25519 Signature
pub struct Signature([u8; ED25519_SIGNATURE_LEN]);

impl From<[u8; ED25519_SIGNATURE_LEN]> for Signature {
    fn from(value: [u8; ED25519_SIGNATURE_LEN]) -> Self {
        Self(value)
    }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Signature {
    pub fn to_base64(&self) -> String {
        BASE64_STANDARD.encode(&self.0)
    }

    pub fn from_base64(base64: &str) -> Option<Self> {
        let mut bytes = [0; ED25519_SIGNATURE_LEN];

        match BASE64_STANDARD.decode_slice_unchecked(base64, &mut bytes) {
            Err(_) => return None,
            _ => {}
        }

        Some(bytes.into())
    }
}

/// Ed25519 Public Key (for verifying signatures)
pub struct PublicKey([u8; ED25519_PUBLIC_KEY_LEN]);

impl From<[u8; ED25519_PUBLIC_KEY_LEN]> for PublicKey {
    fn from(value: [u8; ED25519_PUBLIC_KEY_LEN]) -> Self {
        Self(value)
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl PublicKey {
    pub fn to_base64(&self) -> String {
        BASE64_STANDARD.encode(&self.0)
    }

    pub fn from_base64(base64: &str) -> Option<Self> {
        let mut bytes = [0; ED25519_PUBLIC_KEY_LEN];

        match BASE64_STANDARD.decode_slice_unchecked(base64, &mut bytes) {
            Err(_) => return None,
            _ => {}
        }

        Some(bytes.into())
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let public_key = UnparsedPublicKey::new(&ED25519, &self.0);

        public_key.verify(message, signature.as_ref()).is_ok()
    }
}

/// Ed25519 Key Pair
pub struct KeyPair(Ed25519KeyPair);

impl KeyPair {
    pub fn generate_pkcs8() -> Option<Vec<u8>> {
        let rng = SystemRandom::new();

        let pkcs8 = match Ed25519KeyPair::generate_pkcs8(&rng) {
            Ok(it) => it,
            Err(_) => return None,
        };

        Some(pkcs8.as_ref().into())
    }

    pub fn from_pkcs8(document: &[u8]) -> Option<Self> {
        let key_pair = match Ed25519KeyPair::from_pkcs8_maybe_unchecked(document) {
            Ok(it) => it,
            Err(_) => return None,
        };

        Some(Self(key_pair))
    }

    pub fn sign(&self, data: &[u8]) -> Option<Signature> {
        let signature = self.0.sign(data);

        let bytes: [u8; ED25519_SIGNATURE_LEN] = match signature.as_ref().try_into() {
            Ok(it) => it,
            Err(_) => return None,
        };

        Some(bytes.into())
    }

    pub fn public_key(&self) -> Option<PublicKey> {
        use ring::signature::KeyPair;

        let public_key: [u8; ED25519_PUBLIC_KEY_LEN] =
            self.0.public_key().as_ref().try_into().unwrap();

        Some(public_key.into())
    }
}
