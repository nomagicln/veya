use std::path::PathBuf;
use std::sync::Mutex;

use iota_stronghold::{KeyProvider, SnapshotPath, Stronghold};
use zeroize::Zeroizing;

use crate::error::VeyaError;

const CLIENT_NAME: &[u8] = b"veya-client";

/// Hash a password to exactly 32 bytes for Stronghold's KeyProvider.
fn hash_password(password: &[u8]) -> Vec<u8> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    // Produce 32 bytes by hashing in 4 rounds with different seeds
    let mut result = Vec::with_capacity(32);
    for seed in 0u64..4 {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        password.hash(&mut hasher);
        result.extend_from_slice(&hasher.finish().to_le_bytes());
    }
    result
}

/// Encrypted key-value store backed by IOTA Stronghold.
/// Stores API keys with references like `api_key_{config_id}`.
pub struct StrongholdStore {
    stronghold: Mutex<Stronghold>,
    snapshot_path: SnapshotPath,
    key_provider: KeyProvider,
}

impl StrongholdStore {
    /// Open or create a Stronghold vault at `app_data_dir/veya-keys.stronghold`.
    pub fn open(app_data_dir: PathBuf, password: &[u8]) -> Result<Self, VeyaError> {
        std::fs::create_dir_all(&app_data_dir).map_err(|e| {
            VeyaError::StorageError(format!("Failed to create data dir: {e}"))
        })?;

        let file_path = app_data_dir.join("veya-keys.stronghold");
        let snapshot_path = SnapshotPath::from_path(&file_path);
        let key_provider =
            KeyProvider::try_from(Zeroizing::new(hash_password(password))).map_err(|e| {
                VeyaError::StorageError(format!("Failed to create key provider: {e}"))
            })?;

        let stronghold = Stronghold::default();

        // Load existing snapshot if present, then load the client from it.
        // For a fresh store, create a new client.
        if file_path.exists() {
            stronghold
                .load_snapshot(&key_provider, &snapshot_path)
                .map_err(|e| {
                    VeyaError::StorageError(format!("Failed to load stronghold snapshot: {e}"))
                })?;
            // load_client moves the client from the snapshot into the active clients map
            let _ = stronghold.load_client(CLIENT_NAME);
        } else {
            stronghold.create_client(CLIENT_NAME).map_err(|e| {
                VeyaError::StorageError(format!("Failed to create stronghold client: {e}"))
            })?;
        }

        Ok(Self {
            stronghold: Mutex::new(stronghold),
            snapshot_path,
            key_provider,
        })
    }

    /// Store an API key in the encrypted Client Store.
    /// The store key is `api_key_{config_id}`.
    pub fn store_api_key(&self, config_id: &str, key: &str) -> Result<(), VeyaError> {
        let store_key = format!("api_key_{config_id}");

        let stronghold = self.stronghold.lock().map_err(|e| {
            VeyaError::StorageError(format!("Lock poisoned: {e}"))
        })?;

        let client = stronghold.get_client(CLIENT_NAME).map_err(|e| {
            VeyaError::StorageError(format!("Failed to load client: {e}"))
        })?;

        client
            .store()
            .insert(store_key.as_bytes().to_vec(), key.as_bytes().to_vec(), None)
            .map_err(|e| {
                VeyaError::StorageError(format!("Failed to store API key: {e}"))
            })?;

        stronghold
            .commit_with_keyprovider(&self.snapshot_path, &self.key_provider)
            .map_err(|e| {
                VeyaError::StorageError(format!("Failed to save stronghold: {e}"))
            })?;

        Ok(())
    }

    /// Retrieve an API key by config_id. Returns None if not found.
    pub fn get_api_key(&self, config_id: &str) -> Result<Option<String>, VeyaError> {
        let store_key = format!("api_key_{config_id}");

        let stronghold = self.stronghold.lock().map_err(|e| {
            VeyaError::StorageError(format!("Lock poisoned: {e}"))
        })?;

        let client = stronghold.get_client(CLIENT_NAME).map_err(|e| {
            VeyaError::StorageError(format!("Failed to load client: {e}"))
        })?;

        match client.store().get(store_key.as_bytes()) {
            Ok(Some(bytes)) => {
                let value = String::from_utf8(bytes).map_err(|e| {
                    VeyaError::StorageError(format!("Invalid UTF-8 in stored key: {e}"))
                })?;
                Ok(Some(value))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(VeyaError::StorageError(format!(
                "Failed to read API key: {e}"
            ))),
        }
    }

    /// Delete an API key by config_id.
    pub fn delete_api_key(&self, config_id: &str) -> Result<(), VeyaError> {
        let key = format!("api_key_{config_id}");

        let stronghold = self.stronghold.lock().map_err(|e| {
            VeyaError::StorageError(format!("Lock poisoned: {e}"))
        })?;

        let client = stronghold.get_client(CLIENT_NAME).map_err(|e| {
            VeyaError::StorageError(format!("Failed to load client: {e}"))
        })?;

        client
            .store()
            .delete(key.as_bytes())
            .map_err(|e| {
                VeyaError::StorageError(format!("Failed to delete API key: {e}"))
            })?;

        stronghold
            .commit_with_keyprovider(&self.snapshot_path, &self.key_provider)
            .map_err(|e| {
                VeyaError::StorageError(format!("Failed to save stronghold: {e}"))
            })?;

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (StrongholdStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = StrongholdStore::open(dir.path().to_path_buf(), b"test-password").unwrap();
        (store, dir)
    }

    #[test]
    fn store_and_retrieve_api_key() {
        let (store, _dir) = test_store();
        store.store_api_key("cfg1", "sk-abc123").unwrap();
        let key = store.get_api_key("cfg1").unwrap();
        assert_eq!(key, Some("sk-abc123".to_string()));
    }

    #[test]
    fn get_nonexistent_key_returns_none() {
        let (store, _dir) = test_store();
        let key = store.get_api_key("nonexistent").unwrap();
        assert_eq!(key, None);
    }

    #[test]
    fn delete_api_key() {
        let (store, _dir) = test_store();
        store.store_api_key("cfg2", "sk-xyz789").unwrap();
        assert!(store.get_api_key("cfg2").unwrap().is_some());
        store.delete_api_key("cfg2").unwrap();
        assert_eq!(store.get_api_key("cfg2").unwrap(), None);
    }

    #[test]
    fn overwrite_api_key() {
        let (store, _dir) = test_store();
        store.store_api_key("cfg3", "old-key").unwrap();
        store.store_api_key("cfg3", "new-key").unwrap();
        assert_eq!(store.get_api_key("cfg3").unwrap(), Some("new-key".to_string()));
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Store a key
        {
            let store = StrongholdStore::open(path.clone(), b"pw").unwrap();
            store.store_api_key("persist", "my-secret").unwrap();
        }

        // Reopen and verify
        {
            let store = StrongholdStore::open(path, b"pw").unwrap();
            assert_eq!(store.get_api_key("persist").unwrap(), Some("my-secret".to_string()));
        }
    }
}
