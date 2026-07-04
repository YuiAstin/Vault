# Vault

A lightweight, offline-first password manager built with Tauri 2 (Rust + TypeScript). No cloud, no subscriptions, no telemetry. Your passwords stay on your machine, encrypted at rest.

## Trust & Disclaimer

**This project has not been independently audited.** If you're considering using Vault for sensitive credentials, do your own research first:

- Read the source code, particularly `crypto.rs` and `vault.rs`
- Understand the encryption scheme (Argon2id key derivation + AES-256-GCM)
- Evaluate whether the security model fits your threat model
- Consider that established alternatives (Bitwarden, KeePass) have years of community review and formal audits

This is a personal project made public. Use it at your own risk. I make no guarantees about its security for protecting high-value credentials.

## Features

- **Encryption**: AES-256-GCM with Argon2id key derivation, explicit memory zeroizing
- **Lightweight**: ~9 MB native binary, ~15 MB RAM idle (no Electron)
- **Offline-first**: No account, no server, no internet required
- **Browser extension**: Chrome and Firefox autofill with domain matching
- **Desktop integration**: Global hotkey (Ctrl+Shift+V), auto-type, system tray
- **Security**: Auto-lock on idle, lock on Windows lock screen, clipboard exclusion from history, auto-clear after 15s
- **Organization**: Categories, search, sort, favicons
- **Sync**: Encrypted file sync via any cloud drive (Google Drive, OneDrive, USB)
- **Import**: CSV import from Bitwarden, Chrome, Firefox
- **Breach check**: Have I Been Pwned API (k-Anonymity, your password never leaves your machine)
- **Themes**: 8 built-in presets + fully custom color picker

## Architecture

```
app/
  src/            TypeScript frontend (Vite)
  src-tauri/      Rust backend
    src/
      commands.rs   Tauri IPC commands
      crypto.rs     AES-GCM + Argon2id
      vault.rs      Vault file I/O
      api_server.rs Local API for browser extension
      wts_monitor.rs  Windows session lock detection
extension/
  chrome/         Manifest V3 extension
  firefox/        Firefox port
```

## How It Works

1. Master password is derived into a 256-bit key via Argon2id (salt stored in `vault.meta.json`)
2. Vault data is encrypted/decrypted with AES-256-GCM
3. Decrypted data lives in memory only while unlocked, zeroed on lock
4. Browser extension communicates over localhost:7890 with a per-session auth token
5. Clipboard writes use Win32 API with `ExcludeClipboardContentFromMonitorProcessing` flag (won't appear in Win+V history)

## Build

### Prerequisites

- [Rust](https://rustup.rs/) (1.77+)
- [Node.js](https://nodejs.org/) (18+)
- Windows 10/11 (primary target)

### Development

```bash
cd app
npm install
npm run tauri dev
```

### Production Build

```bash
cd app
npm run tauri build
```

Output: `app/src-tauri/target/release/vault.exe` (~9 MB)

### Browser Extension

1. Open `chrome://extensions` (or `about:addons` for Firefox)
2. Enable developer mode
3. Load unpacked from `extension/chrome/` or `extension/firefox/`
4. Copy your API token from Vault (key icon in header) and paste in extension popup

## Security Model

| Threat | Protection |
|--------|-----------|
| Stolen laptop / disk | AES-256-GCM encryption at rest |
| Weak master password | Argon2id makes brute force expensive |
| Clipboard sniffing | 15s auto-clear + Win32 history exclusion |
| Shoulder surfing | Passwords masked, show on demand |
| Credential reuse | Password generator, breach check |
| Idle machine access | Auto-lock after 5 min, lock on Win+L |
| Casual keylogger | Browser extension fills without keystrokes |

| Threat | NOT protected |
|--------|--------------|
| Kernel-level malware | Can read process memory |
| Admin-level RAT | Can hook any API |
| Physical access while unlocked | Auto-lock helps but isn't instant |

## License

MIT
