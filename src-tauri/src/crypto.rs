use aes_gcm::{
    aead::{Aead, Generate, Key, KeyInit},
    Aes256Gcm, Nonce,
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;

use crate::credential_store;

// Keep the finished demo build separate from credentials authorized for older builds.
const SERVICE: &str = "com.vedant.kyra.ai.v2";
const ACCOUNT: &str = "application-data-key";

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Kyra could not access its encryption key.")]
    Keychain,
    #[error("Kyra's local encryption key is missing. Reset local AI data to continue.")]
    MissingKey,
    #[error("Kyra could not encrypt or decrypt local AI data.")]
    Cipher,
}

#[derive(Clone)]
pub struct LocalCipher {
    key: [u8; 32],
}

impl LocalCipher {
    pub fn load_or_create(has_encrypted_data: bool) -> Result<Self, CryptoError> {
        match credential_store::load(SERVICE, ACCOUNT).map_err(|_| CryptoError::Keychain)? {
            Some(secret) => Self::from_bytes(secret),
            None => {
                if has_encrypted_data {
                    return Err(CryptoError::MissingKey);
                }
                let key = Key::<Aes256Gcm>::generate();
                credential_store::store(SERVICE, ACCOUNT, key.as_slice())
                    .map_err(|_| CryptoError::Keychain)?;
                Self::from_bytes(key.as_slice().to_vec())
            }
        }
    }

    fn from_bytes(bytes: Vec<u8>) -> Result<Self, CryptoError> {
        let key: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::Cipher)?;
        Ok(Self { key })
    }

    #[cfg(test)]
    pub fn random() -> Self {
        let key = Key::<Aes256Gcm>::generate();
        Self::from_bytes(key.as_slice().to_vec()).expect("generated key has correct length")
    }

    pub fn encrypt<T: Serialize>(&self, value: &T) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| CryptoError::Cipher)?;
        let nonce = Nonce::generate();
        let plaintext = serde_json::to_vec(value).map_err(|_| CryptoError::Cipher)?;
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|_| CryptoError::Cipher)?;
        Ok((nonce.as_slice().to_vec(), ciphertext))
    }

    pub fn decrypt<T: DeserializeOwned>(
        &self,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<T, CryptoError> {
        let cipher = Aes256Gcm::new_from_slice(&self.key).map_err(|_| CryptoError::Cipher)?;
        let nonce: [u8; 12] = nonce.try_into().map_err(|_| CryptoError::Cipher)?;
        let plaintext = cipher
            .decrypt(&Nonce::from(nonce), ciphertext)
            .map_err(|_| CryptoError::Cipher)?;
        serde_json::from_slice(&plaintext).map_err(|_| CryptoError::Cipher)
    }

    pub fn pseudonymous_id(&self, namespace: &str, value: &str) -> String {
        use hmac::{Hmac, Mac};

        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key)
            .expect("AES-256 application key is a valid HMAC key");
        mac.update(namespace.as_bytes());
        mac.update(&[0]);
        mac.update(value.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_has_unique_nonces_and_rejects_tampering() {
        let cipher = LocalCipher::random();
        let value = vec!["private", "payload"];
        let (nonce_one, encrypted) = cipher.encrypt(&value).unwrap();
        let (nonce_two, _) = cipher.encrypt(&value).unwrap();
        assert_ne!(nonce_one, nonce_two);
        assert_eq!(
            cipher
                .decrypt::<Vec<String>>(&nonce_one, &encrypted)
                .unwrap(),
            value
        );
        let mut tampered = encrypted;
        tampered[0] ^= 1;
        assert!(matches!(
            cipher.decrypt::<Vec<String>>(&nonce_one, &tampered),
            Err(CryptoError::Cipher)
        ));
    }

    #[test]
    fn pseudonymous_ids_are_stable_and_namespaced() {
        let cipher = LocalCipher::random();
        assert_eq!(
            cipher.pseudonymous_id("person", "someone@example.com"),
            cipher.pseudonymous_id("person", "someone@example.com")
        );
        assert_ne!(
            cipher.pseudonymous_id("person", "someone@example.com"),
            cipher.pseudonymous_id("alias", "someone@example.com")
        );
    }
}
