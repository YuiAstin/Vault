# Vault — Password Manager

## Overview

A lightweight, native password manager built with Tauri (Rust backend + web frontend). Designed to be fast, minimal, and secure — a personal alternative to Bitwarden/KeePass without the bloat.

---

## Architecture

```
┌─────────────────────────────────────────────────┐
│                  Tauri App                       │
│                                                 │
│  ┌──────────────┐       ┌────────────────────┐  │
│  │  Frontend    │ IPC   │   Rust Backend     │  │
│  │  (HTML/TS)   │◄─────►│                    │  │
│  │              │       │  - Crypto (AES-256)│  │
│  │  - Unlock    │       │  - Key derivation  │  │
│  │  - Entry list│       │  - Vault I/O       │  │
│  │  - Add/Edit  │       │  - Password gen    │  │
│  │  - Search    │       │  - System tray     │  │
│  └──────────────┘       └────────────────────┘  │
│                                │                │
│                         ┌──────┴──────┐         │
│                         │  vault.enc  │         │
│                         │  (on disk)  │         │
│                         └─────────────┘         │
└─────────────────────────────────────────────────┘
         │
         │ localhost:7890 (future)
         ▼
┌─────────────────────┐
│  Browser Extension  │
│  (Chrome/Firefox)   │
│  - Form detection   │
│  - Autofill         │
│  - Auto-save        │
└─────────────────────┘
```

---

## Security Design

### Encryption
- **Algorithm**: AES-256-GCM (authenticated encryption)
- **Key derivation**: Argon2id (memory-hard, GPU-resistant)
  - 64 MB memory cost
  - 3 iterations
  - 32-byte output key
- **Salt**: 32 bytes, randomly generated per vault
- **Nonce**: 12 bytes, randomly generated per encryption operation

### Vault File
- Location: `%LOCALAPPDATA%/vault-pm/vault.enc`
- Format: 12-byte nonce + AES-256-GCM ciphertext (of JSON)
- Metadata: `vault.meta.json` stores the base64-encoded salt (not secret)

### In-Memory Security
- Master password held in memory only while vault is unlocked
- Key zeroed from memory after each encrypt/decrypt operation (`zeroize` crate)
- Auto-lock on idle (planned)

### Password Generator
- Cryptographically secure random (OS entropy via `rand`)
- Guarantees at least one uppercase, lowercase, digit, and symbol
- Default length: 20 characters
- Configurable length

---

## Tech Stack

| Layer | Technology | Why |
|-------|-----------|-----|
| Backend | Rust | Memory safety, fast crypto, small binary |
| Frontend | TypeScript + Vite | Fast dev, type safety |
| Framework | Tauri 2 | Native webview, tiny footprint (~9MB) |
| Crypto | aes-gcm, argon2 crates | Audited Rust crypto implementations |
| Packaging | NSIS/MSI | Windows installer, ~2MB |

---

## File Structure

```
vault/
├── app/                      # Tauri project root
│   ├── src/                  # Frontend source
│   │   ├── main.ts          # App logic (screens, events, IPC)
│   │   ├── style.css        # Dark theme UI
│   │   └── vite-env.d.ts    # TypeScript env
│   ├── index.html            # Main HTML (3 screens)
│   ├── src-tauri/            # Rust backend
│   │   ├── src/
│   │   │   ├── main.rs      # Entry point
│   │   │   ├── lib.rs       # Tauri setup, command registration
│   │   │   ├── commands.rs  # IPC command handlers
│   │   │   ├── crypto.rs    # AES-256-GCM, Argon2id, key mgmt
│   │   │   └── vault.rs     # Vault data structures, file I/O
│   │   ├── Cargo.toml       # Rust dependencies
│   │   └── tauri.conf.json  # Tauri config (window, bundle)
│   └── package.json          # Node dependencies
├── vault.py                  # CLI version (Python, standalone)
├── .gitignore                # Excludes vault.enc, .vault_salt
└── DESIGN.md                 # This file
```

---

## Current Features (v0.3)

- [x] Create vault with master password
- [x] Unlock/lock vault
- [x] Add/edit/delete entries (name, username, password, URL, notes, category, TOTP)
- [x] List entries with search, category filter, sort
- [x] View entry details with copy buttons
- [x] Copy username/password to clipboard (Win32 secure — hidden from clipboard history)
- [x] Show/hide password toggle
- [x] Generate strong random passwords
- [x] Auto-clear clipboard after 15 seconds
- [x] TOTP 2FA code generation with live countdown
- [x] QR code screen scan for TOTP setup
- [x] NTP time sync for accurate TOTP codes
- [x] Custom themes (8 presets + custom color picker)
- [x] Browser extension (Chrome/Firefox) with tokenless auto-connect
- [x] Auto-save prompt on form submit
- [x] Rate-limit guardrail with breach alert
- [x] Right-click context menu (Copy Password/Username/2FA)
- [x] Global hotkey, auto-type, system tray
- [x] Auto-lock on idle + Windows lock screen
- [x] Start on boot
- [x] Encrypted file sync (push/pull to cloud drives)
- [x] CSV import (Bitwarden/Chrome/Firefox)
- [x] HIBP breach check (k-Anonymity)
- [x] Dark theme UI, ~15MB native exe

---

## TODO

### Priority 1 — Core UX
- [x] Edit existing entries (currently can only add/delete)
- [x] Categories/folders for organizing entries
- [x] Favicon/icon fetch for entries (from URL)
- [x] Entry sorting options (name, date added, most used)
- [x] Vault backup/export (encrypted file you can move)
- [x] Import from CSV (Bitwarden/Chrome/Firefox export format)

### Priority 2 — Security
- [x] Auto-lock after N minutes of inactivity
- [x] System tray — minimize to tray instead of closing
- [x] Lock on Windows lock screen (WTS session change)
- [x] Master password strength meter on creation
- [x] Breach check (Have I Been Pwned API for passwords)
- [x] Vault integrity check on load (detect corruption)

### Priority 3 — Browser Extension
- [x] Chrome Manifest V3 extension skeleton
- [x] Local API server in Tauri (localhost:7890, auth token)
- [x] Extension popup — list entries matching current domain
- [x] Click-to-fill (user picks entry, extension fills form)
- [x] Auto-detect login forms (heuristic: email/password input pairs)
- [x] Auto-save prompt (detect new credentials on form submit)
- [x] Firefox extension port

### Priority 4 — Desktop Integration
- [x] Global hotkey (Ctrl+Shift+V) — open quick search, copy password
- [x] Auto-type (simulate keystrokes for non-browser apps)
  - *Limitation: Electron apps (Discord, Slack, VS Code) reset focus on window re-entry. Auto-type works best with native Win32 apps. For Electron apps, use clipboard copy or the browser extension.*
- [ ] Windows Hello / biometric unlock option
- [x] Start on boot (optional, system tray)

### Priority 5 — Sync & Multi-Device
- [x] Encrypted vault sync via file (Google Drive, OneDrive, USB)
- [x] Conflict resolution (last-write-wins or merge)
- [ ] Mobile companion (maybe React Native or just encrypted file access)

### Priority 6 — Polish
- [x] Custom themes (8 presets + fully custom color picker with live preview)
- [ ] Keyboard navigation (Tab through entries, Enter to copy)
- [ ] Accessibility (screen reader labels, focus management)
- [ ] Update checker (GitHub releases)
- [ ] About/settings page

### Priority 7 — Recovery
- [ ] Recovery key generated on vault creation (128-bit random, shown once)
- [ ] User prompted to write it down / print it — never stored in plaintext
- [ ] Recovery key can unlock vault and set a new master password
- [ ] Implementation: encrypt the vault key with BOTH the master password AND the recovery key separately (two encrypted copies of the same symmetric key)
- [ ] "Forgot password?" flow in unlock screen — asks for recovery key instead

### Priority 8 — 2FA / TOTP
- [x] TOTP code generation (totp-lite crate, RFC 6238 compliant)
- [x] totp_secret field on entries (optional)
- [x] Live 2FA code display with countdown timer in detail panel
- [x] QR code scanning from screen (rqrr + Win32 screen capture)
- [x] NTP time sync on startup (fetches from Google/Cloudflare/Microsoft, corrects for clock drift)
- [x] otpauth:// URI parsing (extracts secret automatically)
- [x] Copy 2FA code button
- [ ] Drag-to-select QR region overlay (needs multi-page Vite build + Tauri event IPC between windows — `window.close()` doesn't work in Tauri webviews without API access)

### Priority 9 — Extension Improvements
- [x] Tokenless localhost API (no setup needed, extension auto-connects)
- [x] Rate-limit guardrail (5 requests/10s, then lockout until pairing)
- [x] Breach alert UI (shows which entries were exposed before lockout, recommends changing ALL passwords)
- [x] Pairing challenge (6-digit code shown in app, enter in extension to re-pair)
- [x] Right-click context menu on entries (Copy Password, Copy Username, Copy 2FA, Open Details)
- [ ] Passkey/WebAuthn support (requires code-signing cert for Windows credential provider, or Chrome extension monkey-patching)

### Priority 10 — Security Hardening
- [x] Secure clipboard (Win32 ExcludeClipboardContentFromMonitorProcessing — hidden from Win+V)
- [ ] Windows Hello / biometric unlock option
- [ ] Code signing certificate (eliminates SmartScreen warning, enables OS-level credential provider)

---

## Design Decisions

### Why Tauri over Electron?
- 9 MB binary vs 150 MB
- 15 MB RAM idle vs 150 MB
- Native webview (no bundled Chromium)
- Rust backend = memory-safe crypto with explicit zeroing

### Why AES-256-GCM over Fernet/ChaCha20?
- GCM provides authenticated encryption (detects tampering)
- AES-NI hardware acceleration on modern CPUs
- Industry standard — same as what Bitwarden uses

### Why Argon2id over PBKDF2?
- Memory-hard: resistant to GPU/ASIC brute-force attacks
- Argon2id combines data-dependent and data-independent memory access
- Winner of the Password Hashing Competition
- PBKDF2 is fine but dated — Argon2 is strictly better

### Why not just use KeePass/Bitwarden?
- Learning project
- Full control over the code
- No cloud dependency
- Lighter than both (~9MB vs KeePass 30MB vs Bitwarden 150MB)
- Custom features (auto-type, hotkeys tuned to personal workflow)

---

## Build & Run

### Development
```bash
cd "F:\Discord bot\vault\app"
npx tauri dev
```

### Production Build
```bash
cd "F:\Discord bot\vault\app"
npx tauri build
```
Output: `src-tauri/target/release/vault.exe`

### Requirements
- Rust 1.77+ (`rustup`)
- Node.js 20+ (`node --version`)
- npm (`npm --version`)

---

## CLI Version

A standalone Python CLI is also available at `vault/vault.py` for quick terminal access:

```bash
python vault.py init          # Create vault
python vault.py add           # Add entry
python vault.py get github    # Copy password to clipboard
python vault.py list          # List all entries
python vault.py gen 24        # Generate password
```

Requires: `pip install cryptography`

---

*Last updated: 2026-07-05*
