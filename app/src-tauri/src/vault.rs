//! Vault data structures and file operations.

use crate::crypto;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A single vault entry (one set of credentials).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub id: String,
    pub name: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub category: String,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub totp_secret: String,
}

/// The decrypted vault contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultData {
    pub entries: HashMap<String, VaultEntry>,
    pub created_at: String,
}

/// Metadata stored alongside the encrypted vault (not secret).
#[derive(Debug, Serialize, Deserialize)]
pub struct VaultMeta {
    pub salt: String, // base64-encoded salt
    pub version: u32,
    #[serde(default)]
    pub sync_folder: Option<String>,
}

/// Get the vault directory path.
pub fn vault_dir() -> PathBuf {
    let mut dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("vault-pm");
    fs::create_dir_all(&dir).ok();
    dir
}

/// Get the vault file path.
pub fn vault_file() -> PathBuf {
    vault_dir().join("vault.enc")
}

/// Get the meta file path.
pub fn meta_file() -> PathBuf {
    vault_dir().join("vault.meta.json")
}

/// Check if a vault exists.
pub fn vault_exists() -> bool {
    vault_file().exists() && meta_file().exists()
}

/// Create a new vault with the given master password.
pub fn create_vault(master_password: &str) -> Result<(), String> {
    let salt = crypto::generate_salt();

    let vault_data = VaultData {
        entries: HashMap::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Encrypt and save
    save_vault(&vault_data, master_password, &salt)?;

    // Save metadata
    let meta = VaultMeta {
        salt: BASE64.encode(salt),
        version: 1,
        sync_folder: None,
    };
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    fs::write(meta_file(), meta_json).map_err(|e| e.to_string())?;

    Ok(())
}

/// Load and decrypt the vault.
pub fn load_vault(master_password: &str) -> Result<VaultData, String> {
    let meta_json = fs::read_to_string(meta_file()).map_err(|e| e.to_string())?;
    let meta: VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;

    let salt_bytes = BASE64.decode(&meta.salt).map_err(|e| e.to_string())?;
    let mut salt = [0u8; 32];
    salt.copy_from_slice(&salt_bytes);

    let encrypted = fs::read(vault_file()).map_err(|e| e.to_string())?;
    let mut key = crypto::derive_key(master_password, &salt);
    let decrypted = crypto::decrypt(&encrypted, &key)?;
    crypto::zero_key(&mut key);

    let data: VaultData = serde_json::from_slice(&decrypted).map_err(|e| e.to_string())?;
    Ok(data)
}

/// Encrypt and save the vault.
pub fn save_vault(data: &VaultData, master_password: &str, salt: &[u8; 32]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    let mut key = crypto::derive_key(master_password, salt);
    let encrypted = crypto::encrypt(json.as_bytes(), &key);
    crypto::zero_key(&mut key);

    fs::write(vault_file(), encrypted).map_err(|e| e.to_string())?;
    Ok(())
}

/// Save vault with password (reads salt from meta file).
pub fn save_vault_with_password(data: &VaultData, master_password: &str) -> Result<(), String> {
    let meta_json = fs::read_to_string(meta_file()).map_err(|e| e.to_string())?;
    let meta: VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;

    let salt_bytes = BASE64.decode(&meta.salt).map_err(|e| e.to_string())?;
    let mut salt = [0u8; 32];
    salt.copy_from_slice(&salt_bytes);

    save_vault(data, master_password, &salt)
}
