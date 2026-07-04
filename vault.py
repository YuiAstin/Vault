"""
Vault — A minimal password manager.
Usage: python vault.py [command]

Commands:
  init          Create a new vault (set master password)
  unlock        Unlock vault for this session
  add           Add a new entry
  get <name>    Get an entry (copies password to clipboard)
  list          List all entry names
  delete <name> Delete an entry
  gen [length]  Generate a random password (default: 20)
  export        Export all entries (decrypted) to stdout
  change-pw     Change the master password
"""

import base64
import hashlib
import json
import os
import secrets
import string
import sys
import time
import getpass
import threading
from pathlib import Path

from cryptography.fernet import Fernet, InvalidToken
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

# --- Config ---
VAULT_DIR = Path(__file__).parent
VAULT_FILE = VAULT_DIR / "vault.enc"
SALT_FILE = VAULT_DIR / ".vault_salt"
CLIPBOARD_CLEAR_SECONDS = 15


# --- Crypto ---
def derive_key(password: str, salt: bytes) -> bytes:
    """Derive a Fernet key from password + salt using PBKDF2."""
    kdf = PBKDF2HMAC(
        algorithm=hashes.SHA256(),
        length=32,
        salt=salt,
        iterations=600_000,
    )
    return base64.urlsafe_b64encode(kdf.derive(password.encode()))


def encrypt_vault(data: dict, password: str, salt: bytes) -> bytes:
    """Encrypt vault data."""
    key = derive_key(password, salt)
    f = Fernet(key)
    return f.encrypt(json.dumps(data, indent=2).encode())


def decrypt_vault(encrypted: bytes, password: str, salt: bytes) -> dict:
    """Decrypt vault data. Raises InvalidToken on wrong password."""
    key = derive_key(password, salt)
    f = Fernet(key)
    decrypted = f.decrypt(encrypted)
    return json.loads(decrypted.decode())


# --- Clipboard ---
def copy_to_clipboard(text: str):
    """Copy text to clipboard (Windows)."""
    import subprocess
    process = subprocess.Popen(["clip"], stdin=subprocess.PIPE)
    process.communicate(text.encode("utf-16le"))


def clear_clipboard_after(seconds: int):
    """Clear clipboard after N seconds in background."""
    def _clear():
        time.sleep(seconds)
        import subprocess
        process = subprocess.Popen(["clip"], stdin=subprocess.PIPE)
        process.communicate(b"")
        print(f"\n[Clipboard cleared after {seconds}s]")
    t = threading.Thread(target=_clear, daemon=True)
    t.start()


# --- Password Generator ---
def generate_password(length: int = 20) -> str:
    """Generate a strong random password."""
    alphabet = string.ascii_letters + string.digits + "!@#$%^&*()-_=+[]{}|;:,.<>?"
    # Ensure at least one of each category
    pw = [
        secrets.choice(string.ascii_uppercase),
        secrets.choice(string.ascii_lowercase),
        secrets.choice(string.digits),
        secrets.choice("!@#$%^&*()-_=+[]{}|;:,.<>?"),
    ]
    pw += [secrets.choice(alphabet) for _ in range(length - 4)]
    # Shuffle
    pw_list = list(pw)
    secrets.SystemRandom().shuffle(pw_list)
    return "".join(pw_list)


# --- Vault Operations ---
def init_vault():
    """Create a new vault."""
    if VAULT_FILE.exists():
        confirm = input("Vault already exists. Overwrite? (yes/no): ")
        if confirm.lower() != "yes":
            print("Aborted.")
            return

    pw = getpass.getpass("Set master password: ")
    pw2 = getpass.getpass("Confirm master password: ")
    if pw != pw2:
        print("Passwords don't match.")
        return

    salt = os.urandom(32)
    SALT_FILE.write_bytes(salt)

    vault_data = {"entries": {}, "created": time.strftime("%Y-%m-%d %H:%M:%S")}
    encrypted = encrypt_vault(vault_data, pw, salt)
    VAULT_FILE.write_bytes(encrypted)

    print("Vault created successfully.")


def load_vault() -> tuple[dict, str, bytes]:
    """Unlock and load vault. Returns (data, password, salt)."""
    if not VAULT_FILE.exists():
        print("No vault found. Run 'python vault.py init' first.")
        sys.exit(1)

    salt = SALT_FILE.read_bytes()
    pw = getpass.getpass("Master password: ")

    try:
        data = decrypt_vault(VAULT_FILE.read_bytes(), pw, salt)
    except InvalidToken:
        print("Wrong password.")
        sys.exit(1)

    return data, pw, salt


def save_vault(data: dict, password: str, salt: bytes):
    """Save vault back to disk."""
    encrypted = encrypt_vault(data, password, salt)
    VAULT_FILE.write_bytes(encrypted)


def add_entry():
    """Add a new entry to the vault."""
    data, pw, salt = load_vault()

    name = input("Entry name (e.g. 'github', 'steam'): ").strip()
    if not name:
        print("Name cannot be empty.")
        return
    if name in data["entries"]:
        overwrite = input(f"'{name}' already exists. Overwrite? (yes/no): ")
        if overwrite.lower() != "yes":
            print("Aborted.")
            return

    username = input("Username/email: ").strip()

    use_gen = input("Generate password? (yes/no): ").strip().lower()
    if use_gen == "yes":
        length = input("Length (default 20): ").strip()
        length = int(length) if length else 20
        entry_pw = generate_password(length)
        print(f"Generated: {entry_pw}")
        copy_to_clipboard(entry_pw)
        clear_clipboard_after(CLIPBOARD_CLEAR_SECONDS)
        print(f"(Copied to clipboard, clears in {CLIPBOARD_CLEAR_SECONDS}s)")
    else:
        entry_pw = getpass.getpass("Password: ")

    url = input("URL (optional): ").strip()
    notes = input("Notes (optional): ").strip()

    data["entries"][name] = {
        "username": username,
        "password": entry_pw,
        "url": url,
        "notes": notes,
        "added": time.strftime("%Y-%m-%d %H:%M:%S"),
    }

    save_vault(data, pw, salt)
    print(f"Entry '{name}' saved.")


def get_entry(name: str):
    """Retrieve an entry and copy password to clipboard."""
    data, pw, salt = load_vault()

    if name not in data["entries"]:
        print(f"No entry named '{name}'.")
        # Suggest similar
        matches = [k for k in data["entries"] if name.lower() in k.lower()]
        if matches:
            print(f"Did you mean: {', '.join(matches)}?")
        return

    entry = data["entries"][name]
    print(f"\n  Name:     {name}")
    print(f"  Username: {entry['username']}")
    print(f"  Password: {'*' * len(entry['password'])}")
    if entry.get("url"):
        print(f"  URL:      {entry['url']}")
    if entry.get("notes"):
        print(f"  Notes:    {entry['notes']}")
    print(f"  Added:    {entry.get('added', 'unknown')}")

    copy_to_clipboard(entry["password"])
    clear_clipboard_after(CLIPBOARD_CLEAR_SECONDS)
    print(f"\n  Password copied to clipboard (clears in {CLIPBOARD_CLEAR_SECONDS}s)")


def list_entries():
    """List all entry names."""
    data, pw, salt = load_vault()

    entries = data["entries"]
    if not entries:
        print("Vault is empty.")
        return

    print(f"\n  Vault ({len(entries)} entries):")
    print("  " + "-" * 40)
    for name, entry in sorted(entries.items()):
        user = entry.get("username", "")
        print(f"  {name:<20} {user}")
    print()


def delete_entry(name: str):
    """Delete an entry."""
    data, pw, salt = load_vault()

    if name not in data["entries"]:
        print(f"No entry named '{name}'.")
        return

    confirm = input(f"Delete '{name}'? (yes/no): ")
    if confirm.lower() != "yes":
        print("Aborted.")
        return

    del data["entries"][name]
    save_vault(data, pw, salt)
    print(f"Entry '{name}' deleted.")


def export_entries():
    """Export all entries decrypted to stdout."""
    data, pw, salt = load_vault()

    entries = data["entries"]
    if not entries:
        print("Vault is empty.")
        return

    print("\n--- VAULT EXPORT (PLAINTEXT) ---")
    for name, entry in sorted(entries.items()):
        print(f"\n[{name}]")
        print(f"  Username: {entry['username']}")
        print(f"  Password: {entry['password']}")
        if entry.get("url"):
            print(f"  URL:      {entry['url']}")
        if entry.get("notes"):
            print(f"  Notes:    {entry['notes']}")
    print("\n--- END EXPORT ---")


def change_password():
    """Change the master password."""
    data, old_pw, salt = load_vault()

    new_pw = getpass.getpass("New master password: ")
    new_pw2 = getpass.getpass("Confirm new password: ")
    if new_pw != new_pw2:
        print("Passwords don't match.")
        return

    # Generate new salt for extra security
    new_salt = os.urandom(32)
    SALT_FILE.write_bytes(new_salt)
    save_vault(data, new_pw, new_salt)
    print("Master password changed.")


def gen_password(length: int = 20):
    """Generate and display a password."""
    pw = generate_password(length)
    print(f"\n  Generated: {pw}")
    copy_to_clipboard(pw)
    clear_clipboard_after(CLIPBOARD_CLEAR_SECONDS)
    print(f"  (Copied to clipboard, clears in {CLIPBOARD_CLEAR_SECONDS}s)\n")


# --- Main ---
def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return

    cmd = sys.argv[1].lower()

    if cmd == "init":
        init_vault()
    elif cmd == "add":
        add_entry()
    elif cmd == "get":
        if len(sys.argv) < 3:
            print("Usage: vault.py get <name>")
            return
        get_entry(sys.argv[2])
    elif cmd == "list":
        list_entries()
    elif cmd == "delete":
        if len(sys.argv) < 3:
            print("Usage: vault.py delete <name>")
            return
        delete_entry(sys.argv[2])
    elif cmd == "gen":
        length = int(sys.argv[2]) if len(sys.argv) > 2 else 20
        gen_password(length)
    elif cmd == "export":
        export_entries()
    elif cmd == "change-pw":
        change_password()
    elif cmd == "unlock":
        data, pw, salt = load_vault()
        print(f"Vault unlocked. {len(data['entries'])} entries.")
    else:
        print(f"Unknown command: {cmd}")
        print(__doc__)


if __name__ == "__main__":
    main()
