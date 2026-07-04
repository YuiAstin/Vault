//! Cryptographic operations for the vault.
//! Uses Argon2id for key derivation and AES-256-GCM for encryption.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Argon2, Algorithm, Params, Version};
use rand::RngCore;
use zeroize::Zeroize;

/// Derive a 32-byte key from password + salt using Argon2id.
pub fn derive_key(password: &str, salt: &[u8; 32]) -> [u8; 32] {
    let params = Params::new(65536, 3, 1, Some(32)).unwrap(); // 64MB memory, 3 iterations
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .expect("Argon2 hashing failed");
    key
}

/// Encrypt plaintext with AES-256-GCM. Returns nonce + ciphertext.
pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();

    // Generate random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext).expect("Encryption failed");

    // Prepend nonce to ciphertext
    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    result
}

/// Decrypt ciphertext (nonce + encrypted data) with AES-256-GCM.
pub fn decrypt(data: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if data.len() < 12 {
        return Err("Data too short".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — wrong password or corrupted data".to_string())
}

/// Generate a random salt.
pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

/// Generate a random password of given length.
pub fn generate_password(length: usize) -> String {
    use rand::Rng;

    let uppercase = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let lowercase = b"abcdefghijklmnopqrstuvwxyz";
    let digits = b"0123456789";
    let symbols = b"!@#$%^&*()-_=+[]{}|;:,.<>?";

    let all: Vec<u8> = [uppercase.as_slice(), lowercase, digits, symbols].concat();
    let mut rng = rand::thread_rng();

    let mut password: Vec<u8> = Vec::with_capacity(length);

    // Ensure at least one of each category
    password.push(uppercase[rng.gen_range(0..uppercase.len())]);
    password.push(lowercase[rng.gen_range(0..lowercase.len())]);
    password.push(digits[rng.gen_range(0..digits.len())]);
    password.push(symbols[rng.gen_range(0..symbols.len())]);

    // Fill the rest
    for _ in 4..length {
        password.push(all[rng.gen_range(0..all.len())]);
    }

    // Shuffle
    for i in (1..password.len()).rev() {
        let j = rng.gen_range(0..=i);
        password.swap(i, j);
    }

    String::from_utf8(password).unwrap()
}

/// Securely zero a key from memory.
pub fn zero_key(key: &mut [u8; 32]) {
    key.zeroize();
}
