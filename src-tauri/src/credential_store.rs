#[cfg(target_os = "macos")]
mod platform {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use security_framework::os::macos::{
        keychain::{CreateOptions, KeychainSettings, SecKeychain},
        passwords::SecKeychainItemPassword,
    };

    const KEYCHAIN_PASSWORD: &str = "kyra-local-secrets-v1";
    const ITEM_NOT_FOUND: i32 = -25_300;
    static STORE_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Debug, thiserror::Error)]
    pub enum CredentialStoreError {
        #[error("Kyra could not access its private credential keychain.")]
        Unavailable,
    }

    fn path() -> Result<PathBuf, CredentialStoreError> {
        let home = std::env::var_os("HOME").ok_or(CredentialStoreError::Unavailable)?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("com.vedant.kyra")
            .join("kyra-secrets.keychain-db"))
    }

    fn open(path: &Path) -> Result<SecKeychain, CredentialStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| CredentialStoreError::Unavailable)?;
        }
        let mut keychain = if path.exists() {
            SecKeychain::open(path).map_err(|_| CredentialStoreError::Unavailable)?
        } else {
            CreateOptions::new()
                .password(KEYCHAIN_PASSWORD)
                .prompt_user(false)
                .create(path)
                .map_err(|_| CredentialStoreError::Unavailable)?
        };
        keychain
            .unlock(Some(KEYCHAIN_PASSWORD))
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let mut settings = KeychainSettings::new();
        settings.set_lock_on_sleep(false);
        settings.set_lock_interval(None);
        keychain
            .set_settings(&settings)
            .map_err(|_| CredentialStoreError::Unavailable)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|_| CredentialStoreError::Unavailable)?;
        Ok(keychain)
    }

    fn secret_bytes(secret: SecKeychainItemPassword) -> Vec<u8> {
        secret.as_ref().to_vec()
    }

    fn load_at(
        keychain_path: &Path,
        service: &str,
        account: &str,
    ) -> Result<Option<Vec<u8>>, CredentialStoreError> {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let _no_ui = SecKeychain::disable_user_interaction()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let keychain = open(keychain_path)?;
        match keychain.find_generic_password(service, account) {
            Ok((secret, _)) => Ok(Some(secret_bytes(secret))),
            Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
            Err(_) => Err(CredentialStoreError::Unavailable),
        }
    }

    fn store_at(
        keychain_path: &Path,
        service: &str,
        account: &str,
        secret: &[u8],
    ) -> Result<(), CredentialStoreError> {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let _no_ui = SecKeychain::disable_user_interaction()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let keychain = open(keychain_path)?;
        match keychain.find_generic_password(service, account) {
            Ok((_, mut item)) => item
                .set_password(secret)
                .map_err(|_| CredentialStoreError::Unavailable),
            Err(error) if error.code() == ITEM_NOT_FOUND => keychain
                .add_generic_password(service, account, secret)
                .map_err(|_| CredentialStoreError::Unavailable),
            Err(_) => Err(CredentialStoreError::Unavailable),
        }
    }

    fn delete_at(
        keychain_path: &Path,
        service: &str,
        account: &str,
    ) -> Result<(), CredentialStoreError> {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let _no_ui = SecKeychain::disable_user_interaction()
            .map_err(|_| CredentialStoreError::Unavailable)?;
        let keychain = open(keychain_path)?;
        match keychain.find_generic_password(service, account) {
            Ok((_, item)) => {
                item.delete();
                Ok(())
            }
            Err(error) if error.code() == ITEM_NOT_FOUND => Ok(()),
            Err(_) => Err(CredentialStoreError::Unavailable),
        }
    }

    pub fn load(service: &str, account: &str) -> Result<Option<Vec<u8>>, CredentialStoreError> {
        load_at(&path()?, service, account)
    }

    pub fn store(service: &str, account: &str, secret: &[u8]) -> Result<(), CredentialStoreError> {
        store_at(&path()?, service, account, secret)
    }

    pub fn delete(service: &str, account: &str) -> Result<(), CredentialStoreError> {
        delete_at(&path()?, service, account)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use uuid::Uuid;

        #[test]
        fn dedicated_keychain_round_trip_never_needs_ui() {
            let test_path = std::env::temp_dir().join(format!(
                "kyra-credential-store-{}.keychain-db",
                Uuid::new_v4()
            ));
            assert_eq!(load_at(&test_path, "kyra.test", "account").unwrap(), None);
            store_at(&test_path, "kyra.test", "account", b"first").unwrap();
            assert_eq!(
                load_at(&test_path, "kyra.test", "account").unwrap(),
                Some(b"first".to_vec())
            );
            store_at(&test_path, "kyra.test", "account", b"second").unwrap();
            assert_eq!(
                load_at(&test_path, "kyra.test", "account").unwrap(),
                Some(b"second".to_vec())
            );
            delete_at(&test_path, "kyra.test", "account").unwrap();
            assert_eq!(load_at(&test_path, "kyra.test", "account").unwrap(), None);
            let _ = fs::remove_file(test_path);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    #[derive(Debug, thiserror::Error)]
    pub enum CredentialStoreError {
        #[error("Kyra's credential store is available only on macOS.")]
        Unavailable,
    }

    pub fn load(_service: &str, _account: &str) -> Result<Option<Vec<u8>>, CredentialStoreError> {
        Err(CredentialStoreError::Unavailable)
    }

    pub fn store(
        _service: &str,
        _account: &str,
        _secret: &[u8],
    ) -> Result<(), CredentialStoreError> {
        Err(CredentialStoreError::Unavailable)
    }

    pub fn delete(_service: &str, _account: &str) -> Result<(), CredentialStoreError> {
        Err(CredentialStoreError::Unavailable)
    }
}

pub use platform::{delete, load, store};
