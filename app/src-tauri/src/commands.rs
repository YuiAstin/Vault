//! Tauri commands — these are callable from the frontend via invoke().

use crate::{crypto, vault};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;

/// App state: holds the decrypted vault data and master password while unlocked.
pub struct AppState {
    pub vault_data: Mutex<Option<vault::VaultData>>,
    pub master_password: Mutex<Option<String>>,
    pub api_token: Mutex<Option<String>>,
    pub pairing_code: Mutex<Option<String>>,
    /// Clock offset in seconds (actual_time = system_time + offset)
    pub time_offset: Mutex<i64>,
}

#[derive(Serialize)]
pub struct EntryResponse {
    pub id: String,
    pub name: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub category: String,
    pub created_at: String,
    pub totp_secret: String,
}

#[derive(Serialize)]
pub struct EntryListItem {
    pub id: String,
    pub name: String,
    pub username: String,
    pub url: String,
    pub category: String,
    pub created_at: String,
}

// --- Commands ---

#[tauri::command]
pub fn vault_exists() -> bool {
    vault::vault_exists()
}

#[tauri::command]
pub fn create_vault(password: String) -> Result<(), String> {
    vault::create_vault(&password)
}

#[tauri::command]
pub fn unlock_vault(password: String, state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    let data = vault::load_vault(&password)?;
    let count = data.entries.len();

    *state.vault_data.lock().unwrap() = Some(data);
    *state.master_password.lock().unwrap() = Some(password);

    // Generate API token for browser extension
    let token = uuid::Uuid::new_v4().to_string();
    *state.api_token.lock().unwrap() = Some(token);

    Ok(count)
}

#[tauri::command]
pub fn lock_vault(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    *state.vault_data.lock().unwrap() = None;
    *state.master_password.lock().unwrap() = None;
    *state.api_token.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub fn is_unlocked(state: State<'_, Arc<AppState>>) -> bool {
    state.vault_data.lock().unwrap().is_some()
}

#[tauri::command]
pub fn list_entries(state: State<'_, Arc<AppState>>) -> Result<Vec<EntryListItem>, String> {
    let guard = state.vault_data.lock().unwrap();
    let data = guard.as_ref().ok_or("Vault is locked")?;

    let mut entries: Vec<EntryListItem> = data
        .entries
        .values()
        .map(|e| EntryListItem {
            id: e.id.clone(),
            name: e.name.clone(),
            username: e.username.clone(),
            url: e.url.clone(),
            category: e.category.clone(),
            created_at: e.created_at.clone(),
        })
        .collect();

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

#[tauri::command]
pub fn get_entry(id: String, state: State<'_, Arc<AppState>>) -> Result<EntryResponse, String> {
    let guard = state.vault_data.lock().unwrap();
    let data = guard.as_ref().ok_or("Vault is locked")?;

    let entry = data.entries.get(&id).ok_or("Entry not found")?;
    Ok(EntryResponse {
        id: entry.id.clone(),
        name: entry.name.clone(),
        username: entry.username.clone(),
        password: entry.password.clone(),
        url: entry.url.clone(),
        notes: entry.notes.clone(),
        category: entry.category.clone(),
        created_at: entry.created_at.clone(),
        totp_secret: entry.totp_secret.clone(),
    })
}

#[derive(Deserialize)]
pub struct AddEntryInput {
    pub name: String,
    pub username: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub category: Option<String>,
    pub totp_secret: Option<String>,
}

#[tauri::command]
pub fn get_totp_code(id: String, state: State<'_, Arc<AppState>>) -> Result<TotpResponse, String> {
    let guard = state.vault_data.lock().unwrap();
    let data = guard.as_ref().ok_or("Vault is locked")?;
    let entry = data.entries.get(&id).ok_or("Entry not found")?;

    if entry.totp_secret.is_empty() {
        return Err("No TOTP secret configured for this entry".to_string());
    }

    let time_offset = *state.time_offset.lock().unwrap();
    let code = generate_totp(&entry.totp_secret, time_offset)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64 + time_offset;
    let remaining = 30 - (now % 30) as u8;

    Ok(TotpResponse { code, remaining })
}

#[derive(Serialize)]
pub struct TotpResponse {
    pub code: String,
    pub remaining: u8,
}

/// Generate a TOTP code from a base32-encoded secret or an otpauth:// URI.
fn generate_totp(secret_input: &str, time_offset: i64) -> Result<String, String> {
    use data_encoding::BASE32;

    // Extract secret from otpauth:// URI if needed
    let secret_b32 = if secret_input.starts_with("otpauth://") {
        extract_secret_from_uri(secret_input)?
    } else {
        secret_input.to_string()
    };

    // Clean the secret: remove spaces, dashes, uppercase, strip padding
    let cleaned: String = secret_b32
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
        .collect::<String>()
        .to_uppercase();

    // Pad to valid base32 length (multiple of 8)
    let padded = {
        let remainder = cleaned.len() % 8;
        if remainder == 0 {
            cleaned.clone()
        } else {
            let pad_count = 8 - remainder;
            format!("{}{}", cleaned, "=".repeat(pad_count))
        }
    };

    let secret = BASE32.decode(padded.as_bytes())
        .map_err(|e| format!("Invalid TOTP secret (bad base32): {}", e))?;

    // Get corrected time (system time + NTP offset)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64 + time_offset;

    // Use totp-lite for correct code generation
    let code = totp_lite::totp_custom::<totp_lite::Sha1>(30, 6, &secret, now as u64);

    Ok(code)
}

/// Extract the secret parameter from an otpauth:// URI.
fn extract_secret_from_uri(uri: &str) -> Result<String, String> {
    // otpauth://totp/Label?secret=BASE32SECRET&issuer=Example
    // The secret might be URL-encoded (unlikely for base32, but handle %XX just in case)
    uri.split('?')
        .nth(1)
        .and_then(|query| {
            query.split('&')
                .find(|p| p.to_lowercase().starts_with("secret="))
                .map(|p| {
                    let raw = p.splitn(2, '=').nth(1).unwrap_or("");
                    // URL-decode (base32 chars are safe, but handle edge cases)
                    raw.replace("%3D", "=").replace("%3d", "=")
                })
        })
        .ok_or_else(|| "Invalid otpauth URI: no secret parameter found".to_string())
}

#[tauri::command]
pub fn add_entry(input: AddEntryInput, state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let mut guard = state.vault_data.lock().unwrap();
    let data = guard.as_mut().ok_or("Vault is locked")?;

    let id = uuid::Uuid::new_v4().to_string();
    let entry = vault::VaultEntry {
        id: id.clone(),
        name: input.name,
        username: input.username,
        password: input.password,
        url: input.url.unwrap_or_default(),
        notes: input.notes.unwrap_or_default(),
        category: input.category.unwrap_or_default(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: String::new(),
        totp_secret: input.totp_secret.unwrap_or_default(),
    };

    data.entries.insert(id.clone(), entry);

    // Save to disk
    let pw_guard = state.master_password.lock().unwrap();
    let pw = pw_guard.as_ref().ok_or("No master password")?;
    vault::save_vault_with_password(data, pw)?;

    Ok(id)
}

#[derive(Deserialize)]
pub struct EditEntryInput {
    pub id: String,
    pub name: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub category: Option<String>,
    pub totp_secret: Option<String>,
}

#[tauri::command]
pub fn edit_entry(input: EditEntryInput, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut guard = state.vault_data.lock().unwrap();
    let data = guard.as_mut().ok_or("Vault is locked")?;

    let entry = data.entries.get_mut(&input.id).ok_or("Entry not found")?;

    if let Some(name) = input.name {
        entry.name = name;
    }
    if let Some(username) = input.username {
        entry.username = username;
    }
    if let Some(password) = input.password {
        entry.password = password;
    }
    if let Some(url) = input.url {
        entry.url = url;
    }
    if let Some(notes) = input.notes {
        entry.notes = notes;
    }
    if let Some(category) = input.category {
        entry.category = category;
    }
    if let Some(totp_secret) = input.totp_secret {
        entry.totp_secret = totp_secret;
    }

    entry.updated_at = chrono::Utc::now().to_rfc3339();

    // Save to disk
    let pw_guard = state.master_password.lock().unwrap();
    let pw = pw_guard.as_ref().ok_or("No master password")?;
    vault::save_vault_with_password(data, pw)?;

    Ok(())
}

#[tauri::command]
pub fn list_categories(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let guard = state.vault_data.lock().unwrap();
    let data = guard.as_ref().ok_or("Vault is locked")?;

    let mut categories: Vec<String> = data
        .entries
        .values()
        .map(|e| e.category.clone())
        .filter(|c| !c.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    categories.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    Ok(categories)
}

#[tauri::command]
pub fn delete_entry(id: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut guard = state.vault_data.lock().unwrap();
    let data = guard.as_mut().ok_or("Vault is locked")?;

    data.entries.remove(&id).ok_or("Entry not found")?;

    // Save to disk
    let pw_guard = state.master_password.lock().unwrap();
    let pw = pw_guard.as_ref().ok_or("No master password")?;
    vault::save_vault_with_password(data, pw)?;

    Ok(())
}

#[tauri::command]
pub fn generate_password(length: Option<usize>) -> String {
    crypto::generate_password(length.unwrap_or(20))
}

#[tauri::command]
pub fn check_vault_integrity() -> Result<String, String> {
    if !vault::vault_exists() {
        return Err("No vault found".to_string());
    }

    // Check vault.enc is readable and has valid structure (nonce + ciphertext)
    let vault_data = std::fs::read(vault::vault_file()).map_err(|e| format!("Cannot read vault file: {}", e))?;
    if vault_data.len() < 12 {
        return Err("Vault file is corrupted (too small)".to_string());
    }

    // Check meta file is valid JSON
    let meta_json = std::fs::read_to_string(vault::meta_file())
        .map_err(|e| format!("Cannot read meta file: {}", e))?;
    let _meta: vault::VaultMeta = serde_json::from_str(&meta_json)
        .map_err(|_| "Meta file is corrupted (invalid JSON)".to_string())?;

    Ok(format!("Vault OK ({} bytes)", vault_data.len()))
}

#[tauri::command]
pub fn check_breach(password: String) -> Result<u64, String> {
    use sha1::{Sha1, Digest};

    // SHA-1 hash the password
    let mut hasher = Sha1::new();
    hasher.update(password.as_bytes());
    let hash = format!("{:X}", hasher.finalize());

    // Split: first 5 chars (prefix) and rest (suffix)
    let prefix = &hash[..5];
    let suffix = &hash[5..];

    // Query HIBP range API with k-Anonymity
    let url = format!("https://api.pwnedpasswords.com/range/{}", prefix);
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "Vault-PasswordManager/0.1")
        .send()
        .map_err(|e| format!("Network error: {}", e))?;

    let body = response.text().map_err(|e| format!("Failed to read response: {}", e))?;

    // Each line is "HASH_SUFFIX:COUNT"
    for line in body.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() == 2 && parts[0].eq_ignore_ascii_case(suffix) {
            return Ok(parts[1].trim().parse::<u64>().unwrap_or(0));
        }
    }

    // Not found in breaches
    Ok(0)
}

#[tauri::command]
pub fn get_vault_path() -> String {
    vault::vault_dir().to_string_lossy().to_string()
}

#[tauri::command]
pub fn export_vault(destination: String) -> Result<(), String> {
    let dest = std::path::Path::new(&destination);

    // Copy vault.enc
    let vault_src = vault::vault_file();
    let vault_dest = dest.join("vault.enc");
    std::fs::copy(&vault_src, &vault_dest).map_err(|e| format!("Failed to copy vault: {}", e))?;

    // Copy vault.meta.json
    let meta_src = vault::meta_file();
    let meta_dest = dest.join("vault.meta.json");
    std::fs::copy(&meta_src, &meta_dest).map_err(|e| format!("Failed to copy meta: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn import_vault(source: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let src = std::path::Path::new(&source);

    let vault_src = src.join("vault.enc");
    let meta_src = src.join("vault.meta.json");

    if !vault_src.exists() {
        return Err("vault.enc not found in selected folder".to_string());
    }
    if !meta_src.exists() {
        return Err("vault.meta.json not found in selected folder".to_string());
    }

    // Lock current vault before replacing
    *state.vault_data.lock().unwrap() = None;
    *state.master_password.lock().unwrap() = None;

    // Replace vault files
    std::fs::copy(&vault_src, vault::vault_file())
        .map_err(|e| format!("Failed to import vault: {}", e))?;
    std::fs::copy(&meta_src, vault::meta_file())
        .map_err(|e| format!("Failed to import meta: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn import_csv(file_path: String, state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    let mut guard = state.vault_data.lock().unwrap();
    let data = guard.as_mut().ok_or("Vault is locked")?;

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let mut reader = csv::Reader::from_reader(content.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| format!("Failed to parse CSV headers: {}", e))?
        .clone();

    let header_lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();

    // Detect format by headers
    let name_col = header_lower.iter().position(|h| h == "name");
    let url_col = header_lower
        .iter()
        .position(|h| h == "url" || h == "login_uri" || h == "hostname");
    let username_col = header_lower
        .iter()
        .position(|h| h == "username" || h == "login_username");
    let password_col = header_lower
        .iter()
        .position(|h| h == "password" || h == "login_password");
    let notes_col = header_lower.iter().position(|h| h == "notes" || h == "note");

    if username_col.is_none() || password_col.is_none() {
        return Err(
            "CSV format not recognized. Need at least username and password columns.".to_string(),
        );
    }

    let mut count = 0;
    for result in reader.records() {
        let record = result.map_err(|e| format!("CSV parse error: {}", e))?;

        let name = name_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .to_string();
        let url = url_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .to_string();
        let username = username_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .to_string();
        let password = password_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .to_string();
        let notes = notes_col
            .and_then(|i| record.get(i))
            .unwrap_or("")
            .to_string();

        // Skip empty rows
        if username.is_empty() && password.is_empty() {
            continue;
        }

        // Use URL domain as name if name is empty
        let entry_name = if name.is_empty() {
            if !url.is_empty() {
                url.replace("https://", "")
                    .replace("http://", "")
                    .split('/')
                    .next()
                    .unwrap_or("Unknown")
                    .to_string()
            } else {
                "Imported Entry".to_string()
            }
        } else {
            name
        };

        let id = uuid::Uuid::new_v4().to_string();
        let entry = vault::VaultEntry {
            id: id.clone(),
            name: entry_name,
            username,
            password,
            url,
            notes,
            category: "Imported".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: String::new(),
            totp_secret: String::new(),
        };

        data.entries.insert(id, entry);
        count += 1;
    }

    // Save to disk
    let pw_guard = state.master_password.lock().unwrap();
    let pw = pw_guard.as_ref().ok_or("No master password")?;
    vault::save_vault_with_password(data, pw)?;

    Ok(count)
}

#[tauri::command]
pub fn get_api_token(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let guard = state.api_token.lock().unwrap();
    guard.clone().ok_or("Vault is locked — no token available".to_string())
}

#[tauri::command]
pub fn get_pairing_code(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let guard = state.pairing_code.lock().unwrap();
    guard.clone().ok_or("No pairing code active".to_string())
}

#[tauri::command]
pub fn clear_pairing_code(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut guard = state.pairing_code.lock().unwrap();
    *guard = None;
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn scan_qr_from_screen() -> Result<String, String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDIBits, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY, GetDC,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);

        if width == 0 || height == 0 {
            return Err("Failed to get screen dimensions".to_string());
        }

        // Get screen DC
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("Failed to get screen DC".to_string());
        }

        // Create compatible DC and bitmap for full screen
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        let old_bitmap = SelectObject(mem_dc, bitmap.into());

        // Capture entire screen
        let _ = BitBlt(mem_dc, 0, 0, width, height, Some(screen_dc), 0, 0, SRCCOPY);

        // Prepare bitmap info
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default()],
        };

        let pixel_count = (width * height) as usize;
        let mut pixels: Vec<u8> = vec![0u8; pixel_count * 4];

        let result = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Clean up GDI
        SelectObject(mem_dc, old_bitmap);
        DeleteObject(bitmap.into());
        DeleteDC(mem_dc);
        windows::Win32::Graphics::Gdi::ReleaseDC(None, screen_dc);

        if result == 0 {
            return Err("Failed to capture screen pixels".to_string());
        }

        // Convert BGRA to grayscale
        let mut gray_pixels: Vec<u8> = Vec::with_capacity(pixel_count);
        for i in 0..pixel_count {
            let b = pixels[i * 4] as u32;
            let g = pixels[i * 4 + 1] as u32;
            let r = pixels[i * 4 + 2] as u32;
            let gray = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
            gray_pixels.push(gray);
        }

        // Decode QR
        let mut img = rqrr::PreparedImage::prepare_from_greyscale(
            width as usize,
            height as usize,
            |x, y| gray_pixels[y * width as usize + x],
        );

        let grids = img.detect_grids();
        if grids.is_empty() {
            return Err("No QR code found on screen. Make sure it's fully visible.".to_string());
        }

        for grid in grids {
            match grid.decode() {
                Ok((_meta, content)) => {
                    // If it's an otpauth URI, extract just the secret
                    if content.starts_with("otpauth://") {
                        if let Some(secret) = content.split('?').nth(1)
                            .and_then(|q| q.split('&')
                                .find(|p| p.to_lowercase().starts_with("secret="))
                                .map(|p| p.splitn(2, '=').nth(1).unwrap_or("").to_string()))
                        {
                            return Ok(secret);
                        }
                        return Err("QR contains otpauth URI but no secret parameter".to_string());
                    }
                    return Ok(content);
                }
                Err(_) => continue,
            }
        }

        Err("QR code detected but could not be decoded".to_string())
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn auto_type(text: String, use_tab: bool) -> Result<(), String> {
    use std::thread;
    use std::time::Duration;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VK_TAB,
    };

    // Wait for focus to return to previous app after vault minimizes
    thread::sleep(Duration::from_millis(800));

    fn send_char(c: char) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let code = c as u16;

        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                    wScan: code,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(0),
                    wScan: code,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        }
    }

    fn send_key(vk: u16) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
        }
    }

    // Type each character
    for c in text.chars() {
        if use_tab && c == '\t' {
            send_key(VK_TAB.0);
            thread::sleep(Duration::from_millis(50));
        } else {
            send_char(c);
            thread::sleep(Duration::from_millis(10));
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn set_start_on_boot(enabled: bool) -> Result<(), String> {
    use windows::Win32::System::Registry::{
        RegOpenKeyExW, RegSetValueExW, RegDeleteValueW, RegCloseKey,
        HKEY_CURRENT_USER, KEY_WRITE, REG_SZ,
    };
    use windows::core::PCWSTR;

    let key_path: Vec<u16> = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "Vault\0".encode_utf16().collect();

    unsafe {
        let mut hkey = std::mem::zeroed();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            Some(0),
            KEY_WRITE,
            &mut hkey,
        );
        if result.is_err() {
            return Err("Failed to open registry key".to_string());
        }

        if enabled {
            // Get the current exe path
            let exe_path = std::env::current_exe()
                .map_err(|e| format!("Cannot get exe path: {}", e))?;
            let exe_str = format!("\"{}\"\0", exe_path.to_string_lossy());
            let exe_wide: Vec<u16> = exe_str.encode_utf16().collect();

            let result = RegSetValueExW(
                hkey,
                PCWSTR(value_name.as_ptr()),
                Some(0),
                REG_SZ,
                Some(&exe_wide.iter().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>()),
            );
            RegCloseKey(hkey);
            if result.is_err() {
                return Err("Failed to set registry value".to_string());
            }
        } else {
            let _ = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
            RegCloseKey(hkey);
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_start_on_boot() -> bool {
    use windows::Win32::System::Registry::{
        RegOpenKeyExW, RegQueryValueExW, RegCloseKey,
        HKEY_CURRENT_USER, KEY_READ, REG_VALUE_TYPE,
    };
    use windows::core::PCWSTR;

    let key_path: Vec<u16> = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "Vault\0".encode_utf16().collect();

    unsafe {
        let mut hkey = std::mem::zeroed();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_path.as_ptr()),
            Some(0),
            KEY_READ,
            &mut hkey,
        );
        if result.is_err() {
            return false;
        }

        let mut value_type = REG_VALUE_TYPE(0);
        let mut size = 0u32;
        let result = RegQueryValueExW(
            hkey,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            None,
            Some(&mut size),
        );
        RegCloseKey(hkey);
        result.is_ok() && size > 0
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn get_foreground_window_title() -> String {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

    unsafe {
        let hwnd = GetForegroundWindow();
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len > 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        }
    }
}

#[tauri::command]
pub fn get_sync_folder() -> Result<Option<String>, String> {
    let meta_json = std::fs::read_to_string(vault::meta_file()).map_err(|e| e.to_string())?;
    let meta: vault::VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;
    Ok(meta.sync_folder)
}

#[tauri::command]
pub fn set_sync_folder(folder: Option<String>) -> Result<(), String> {
    let meta_json = std::fs::read_to_string(vault::meta_file()).map_err(|e| e.to_string())?;
    let mut meta: vault::VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;
    meta.sync_folder = folder;
    let updated = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    std::fs::write(vault::meta_file(), updated).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
pub struct SyncStatus {
    pub local_modified: String,
    pub remote_modified: Option<String>,
    pub remote_exists: bool,
    pub conflict: bool,
}

#[tauri::command]
pub fn sync_status() -> Result<SyncStatus, String> {
    let meta_json = std::fs::read_to_string(vault::meta_file()).map_err(|e| e.to_string())?;
    let meta: vault::VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;

    let local_modified = std::fs::metadata(vault::vault_file())
        .and_then(|m| m.modified())
        .map(|t| format!("{:?}", t))
        .unwrap_or_default();

    let sync_folder = match meta.sync_folder {
        Some(ref f) => f.clone(),
        None => return Ok(SyncStatus {
            local_modified,
            remote_modified: None,
            remote_exists: false,
            conflict: false,
        }),
    };

    let remote_vault = std::path::Path::new(&sync_folder).join("vault.enc");
    if !remote_vault.exists() {
        return Ok(SyncStatus {
            local_modified,
            remote_modified: None,
            remote_exists: false,
            conflict: false,
        });
    }

    let remote_mod = std::fs::metadata(&remote_vault)
        .and_then(|m| m.modified())
        .map(|t| format!("{:?}", t))
        .unwrap_or_default();

    let local_time = std::fs::metadata(vault::vault_file())
        .and_then(|m| m.modified())
        .ok();
    let remote_time = std::fs::metadata(&remote_vault)
        .and_then(|m| m.modified())
        .ok();

    let conflict = match (local_time, remote_time) {
        (Some(l), Some(r)) => l != r, // different timestamps = potential conflict
        _ => false,
    };

    Ok(SyncStatus {
        local_modified,
        remote_modified: Some(remote_mod),
        remote_exists: true,
        conflict,
    })
}

#[tauri::command]
pub fn sync_push() -> Result<String, String> {
    let meta_json = std::fs::read_to_string(vault::meta_file()).map_err(|e| e.to_string())?;
    let meta: vault::VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;

    let sync_folder = meta.sync_folder.ok_or("No sync folder configured")?;
    let dest = std::path::Path::new(&sync_folder);
    std::fs::create_dir_all(dest).map_err(|e| format!("Cannot create sync folder: {}", e))?;

    std::fs::copy(vault::vault_file(), dest.join("vault.enc"))
        .map_err(|e| format!("Failed to push vault: {}", e))?;
    std::fs::copy(vault::meta_file(), dest.join("vault.meta.json"))
        .map_err(|e| format!("Failed to push meta: {}", e))?;

    Ok("Pushed to sync folder".to_string())
}

#[tauri::command]
pub fn sync_pull(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let meta_json = std::fs::read_to_string(vault::meta_file()).map_err(|e| e.to_string())?;
    let meta: vault::VaultMeta = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;

    let sync_folder = meta.sync_folder.ok_or("No sync folder configured")?;
    let src = std::path::Path::new(&sync_folder);

    let remote_vault = src.join("vault.enc");
    let remote_meta = src.join("vault.meta.json");

    if !remote_vault.exists() {
        return Err("No vault found in sync folder".to_string());
    }

    // Lock current vault
    *state.vault_data.lock().unwrap() = None;
    *state.master_password.lock().unwrap() = None;
    *state.api_token.lock().unwrap() = None;

    // Replace local with remote
    std::fs::copy(&remote_vault, vault::vault_file())
        .map_err(|e| format!("Failed to pull vault: {}", e))?;
    if remote_meta.exists() {
        std::fs::copy(&remote_meta, vault::meta_file())
            .map_err(|e| format!("Failed to pull meta: {}", e))?;
    }

    Ok("Pulled from sync folder — please unlock with your password".to_string())
}

#[tauri::command]
pub fn copy_to_clipboard_secure(text: String) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::core::HSTRING;

    unsafe {
        // Open clipboard
        OpenClipboard(None).map_err(|e| format!("OpenClipboard failed: {}", e))?;

        EmptyClipboard().map_err(|e| {
            let _ = CloseClipboard();
            format!("EmptyClipboard failed: {}", e)
        })?;

        // Write the text as CF_UNICODETEXT (format 13)
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_len = wide.len() * 2;

        let hmem = GlobalAlloc(GMEM_MOVEABLE, byte_len).map_err(|e| {
            let _ = CloseClipboard();
            format!("GlobalAlloc failed: {}", e)
        })?;

        let ptr = GlobalLock(hmem) as *mut u8;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return Err("GlobalLock returned null".to_string());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr, byte_len);
        let _ = GlobalUnlock(hmem);

        // CF_UNICODETEXT = 13
        SetClipboardData(13, Some(HANDLE(hmem.0))).map_err(|e| {
            let _ = CloseClipboard();
            format!("SetClipboardData (text) failed: {}", e)
        })?;

        // Now set the ExcludeClipboardContentFromMonitorProcessing flag.
        // This is a custom format name recognized by Windows 10+ clipboard history.
        let format_name = HSTRING::from("ExcludeClipboardContentFromMonitorProcessing");
        let exclude_format = RegisterClipboardFormatW(&format_name);
        if exclude_format != 0 {
            // Allocate a minimal buffer (value doesn't matter, just the format's presence)
            let flag_mem = GlobalAlloc(GMEM_MOVEABLE, 1).map_err(|e| {
                let _ = CloseClipboard();
                format!("GlobalAlloc (flag) failed: {}", e)
            })?;

            let flag_ptr = GlobalLock(flag_mem) as *mut u8;
            if !flag_ptr.is_null() {
                *flag_ptr = 0;
                let _ = GlobalUnlock(flag_mem);
            }

            // If this fails, we still have the text on clipboard — not fatal
            let _ = SetClipboardData(exclude_format, Some(HANDLE(flag_mem.0)));
        }

        let _ = CloseClipboard();
    }

    Ok(())
}

#[tauri::command]
pub fn clear_clipboard() -> Result<(), String> {
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};

    unsafe {
        OpenClipboard(None).map_err(|e| format!("OpenClipboard failed: {}", e))?;
        let _ = EmptyClipboard();
        let _ = CloseClipboard();
    }
    Ok(())
}
