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

## Current Features (v0.1)

- [x] Create vault with master password
- [x] Unlock/lock vault
- [x] Add entries (name, username, password, URL, notes)
- [x] List entries with search
- [x] View entry details
- [x] Copy username/password to clipboard
- [x] Show/hide password toggle
- [x] Delete entries
- [x] Generate strong random passwords
- [x] Auto-clear clipboard after 15 seconds
- [x] Dark theme UI
- [x] Builds to ~9MB native exe (Windows)

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
- [ ] Custom themes (light mode option)
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

*Last updated: 2026-07-03*
