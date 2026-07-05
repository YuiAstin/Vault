import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { PRESETS, loadSavedTheme, saveTheme, applyTheme, createCustomTheme, type ThemeConfig, type ThemeColors } from "./theme";

// --- State ---
let currentEntryId: string | null = null;
let currentEntryPassword: string = "";
let allEntries: Array<{ id: string; name: string; username: string; url: string; category: string; created_at: string }> = [];

// --- DOM Elements ---
const authScreen = document.getElementById("auth-screen")!;
const vaultScreen = document.getElementById("vault-screen")!;
const addScreen = document.getElementById("add-screen")!;
const editScreen = document.getElementById("edit-screen")!;
const themeScreen = document.getElementById("theme-screen")!;
const detailPanel = document.getElementById("detail-panel")!;

const authForm = document.getElementById("auth-form") as HTMLFormElement;
const masterPasswordInput = document.getElementById("master-password") as HTMLInputElement;
const confirmPasswordInput = document.getElementById("confirm-password") as HTMLInputElement;
const authBtn = document.getElementById("auth-btn")!;
const authSubtitle = document.getElementById("auth-subtitle")!;
const authError = document.getElementById("auth-error")!;

const entryList = document.getElementById("entry-list")!;
const searchInput = document.getElementById("search-input") as HTMLInputElement;

const addForm = document.getElementById("add-form") as HTMLFormElement;
const genPwBtn = document.getElementById("gen-pw-btn")!;
const entryPasswordInput = document.getElementById("entry-password") as HTMLInputElement;

// --- Screens ---
function showScreen(screen: HTMLElement) {
  [authScreen, vaultScreen, addScreen, editScreen, themeScreen].forEach((s) => s.classList.remove("active"));
  screen.classList.add("active");
}

// --- Toast ---
function toast(msg: string) {
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = msg;
  document.body.appendChild(el);
  setTimeout(() => {
    el.classList.add("fade-out");
    el.addEventListener("animationend", () => el.remove());
  }, 1800);
}

// --- Clipboard ---
async function copyToClipboard(text: string) {
  try {
    await invoke("copy_to_clipboard_secure", { text });
  } catch {
    // Fallback to web API if Rust command fails
    await navigator.clipboard.writeText(text);
  }
  toast("Copied to clipboard");
  // Auto-clear after 15s via Rust (no browser permission needed)
  setTimeout(async () => {
    try {
      await invoke("clear_clipboard");
    } catch {
      // Silent fail — clipboard may have been overwritten by user
    }
  }, 15000);
}

// --- Auth ---
async function initAuth() {
  const exists: boolean = await invoke("vault_exists");
  if (exists) {
    authSubtitle.textContent = "Enter master password";
    authBtn.textContent = "Unlock";
    confirmPasswordInput.style.display = "none";
    // Check vault integrity
    try {
      await invoke("check_vault_integrity");
    } catch (err) {
      authError.textContent = "Warning: " + String(err);
    }
  } else {
    authSubtitle.textContent = "Create a new vault";
    authBtn.textContent = "Create Vault";
    confirmPasswordInput.style.display = "block";
  }
}

authForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  authError.textContent = "";
  const password = masterPasswordInput.value;

  const exists: boolean = await invoke("vault_exists");

  if (!exists) {
    // Creating new vault
    const confirm = confirmPasswordInput.value;
    if (password !== confirm) {
      authError.textContent = "Passwords don't match";
      return;
    }
    if (password.length < 4) {
      authError.textContent = "Password too short (min 4 characters)";
      return;
    }
    try {
      await invoke("create_vault", { password });
      await invoke("unlock_vault", { password });
      await loadEntries();
      showScreen(vaultScreen);
    } catch (err) {
      authError.textContent = String(err);
    }
  } else {
    // Unlocking existing vault
    try {
      await invoke("unlock_vault", { password });
      await loadEntries();
      showScreen(vaultScreen);
    } catch (err) {
      authError.textContent = "Wrong password";
    }
  }
  masterPasswordInput.value = "";
  confirmPasswordInput.value = "";
});

// --- Entry List ---
async function loadEntries() {
  allEntries = await invoke("list_entries");
  renderEntries(allEntries);
  await loadCategories();
}

function renderEntries(entries: typeof allEntries) {
  if (entries.length === 0) {
    entryList.innerHTML = `
      <div class="empty-state">
        <p>No entries yet</p>
        <p>Click + to add your first password</p>
      </div>`;
    return;
  }

  entryList.innerHTML = entries
    .map(
      (entry) => `
    <div class="entry-item" data-id="${entry.id}">
      <div class="entry-icon">${entry.url ? `<img src="https://www.google.com/s2/favicons?domain=${encodeURIComponent(entry.url)}&sz=32" alt="" class="favicon" onerror="this.style.display='none';this.nextElementSibling.style.display='flex'"><span class="favicon-fallback" style="display:none">${entry.name.charAt(0).toUpperCase()}</span>` : entry.name.charAt(0).toUpperCase()}</div>
      <div class="entry-info">
        <div class="name">${escapeHtml(entry.name)}</div>
        <div class="username">${escapeHtml(entry.username)}</div>
      </div>
    </div>`
    )
    .join("");

  // Click handlers
  entryList.querySelectorAll(".entry-item").forEach((el) => {
    el.addEventListener("click", () => {
      const id = (el as HTMLElement).dataset.id!;
      showDetail(id);
    });
    // Right-click context menu
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      const id = (el as HTMLElement).dataset.id!;
      showEntryContextMenu(e as MouseEvent, id);
    });
  });
}

// --- Context Menu ---
function showEntryContextMenu(e: MouseEvent, entryId: string) {
  dismissContextMenu();

  const menu = document.createElement("div");
  menu.id = "entry-context-menu";
  menu.style.cssText = `
    position: fixed; z-index: 300;
    left: ${e.clientX}px; top: ${e.clientY}px;
    background: var(--bg-card); border: 1px solid var(--border);
    border-radius: 8px; padding: 4px 0; min-width: 160px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.3);
    font-size: 13px;
  `;

  const items = [
    { label: "Copy Password", action: "password" },
    { label: "Copy Username", action: "username" },
    { label: "Copy 2FA Code", action: "totp" },
    { label: "Open Details", action: "detail" },
  ];

  items.forEach((item) => {
    const row = document.createElement("div");
    row.textContent = item.label;
    row.style.cssText = `
      padding: 8px 14px; cursor: pointer; color: var(--text);
      transition: background 0.1s;
    `;
    row.addEventListener("mouseenter", () => { row.style.background = "var(--bg-input)"; });
    row.addEventListener("mouseleave", () => { row.style.background = "transparent"; });
    row.addEventListener("click", async () => {
      dismissContextMenu();
      await handleContextAction(entryId, item.action);
    });
    menu.appendChild(row);
  });

  document.body.appendChild(menu);

  // Dismiss on click outside
  setTimeout(() => {
    document.addEventListener("click", dismissContextMenu, { once: true });
  }, 0);
}

function dismissContextMenu() {
  const existing = document.getElementById("entry-context-menu");
  if (existing) existing.remove();
}

async function handleContextAction(entryId: string, action: string) {
  try {
    if (action === "detail") {
      showDetail(entryId);
      return;
    }

    const entry: { username: string; password: string; totp_secret: string } = await invoke("get_entry", { id: entryId });

    if (action === "password") {
      await copyToClipboard(entry.password);
    } else if (action === "username") {
      await copyToClipboard(entry.username);
    } else if (action === "totp") {
      if (!entry.totp_secret) {
        toast("No 2FA configured for this entry");
        return;
      }
      const result: { code: string; remaining: number } = await invoke("get_totp_code", { id: entryId });
      await copyToClipboard(result.code);
      toast(`2FA code copied (${result.remaining}s remaining)`);
      return;
    }
  } catch (err) {
    toast("Error: " + String(err));
  }
}

// --- Search & Filter ---
const categoryFilter = document.getElementById("category-filter") as HTMLSelectElement;
const sortSelect = document.getElementById("sort-select") as HTMLSelectElement;

searchInput.addEventListener("input", () => {
  filterEntries();
});

categoryFilter.addEventListener("change", () => {
  filterEntries();
});

sortSelect.addEventListener("change", () => {
  filterEntries();
});

function filterEntries() {
  const query = searchInput.value.toLowerCase();
  const category = categoryFilter.value;
  const sortMode = sortSelect.value;

  let filtered = allEntries.filter((e) => {
    const matchesSearch =
      !query ||
      e.name.toLowerCase().includes(query) ||
      e.username.toLowerCase().includes(query) ||
      e.url.toLowerCase().includes(query);
    const matchesCategory = !category || e.category === category;
    return matchesSearch && matchesCategory;
  });

  filtered.sort((a, b) => {
    switch (sortMode) {
      case "name-desc":
        return b.name.toLowerCase().localeCompare(a.name.toLowerCase());
      case "newest":
        return b.created_at.localeCompare(a.created_at);
      case "oldest":
        return a.created_at.localeCompare(b.created_at);
      case "name-asc":
      default:
        return a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    }
  });

  renderEntries(filtered);
}

async function loadCategories() {
  const categories: string[] = await invoke("list_categories");
  // Update filter dropdown
  categoryFilter.innerHTML = `<option value="">All Categories</option>` +
    categories.map((c) => `<option value="${escapeHtml(c)}">${escapeHtml(c)}</option>`).join("");
  // Update datalist suggestions for add/edit forms
  const suggestions = categories.map((c) => `<option value="${escapeHtml(c)}">`).join("");
  document.getElementById("category-suggestions")!.innerHTML = suggestions;
  document.getElementById("edit-category-suggestions")!.innerHTML = suggestions;
}

// --- Detail Panel ---
async function showDetail(id: string) {
  try {
    const entry: {
      id: string;
      name: string;
      username: string;
      password: string;
      url: string;
      notes: string;
      category: string;
      created_at: string;
      totp_secret: string;
    } = await invoke("get_entry", { id });

    currentEntryId = entry.id;
    currentEntryPassword = entry.password;

    document.getElementById("detail-name")!.textContent = entry.name;
    document.getElementById("detail-username")!.textContent = entry.username;
    document.getElementById("detail-password")!.textContent = "\u2022".repeat(
      Math.min(entry.password.length, 20)
    );

    const urlField = document.getElementById("detail-url-field")!;
    const categoryField = document.getElementById("detail-category-field")!;
    const notesField = document.getElementById("detail-notes-field")!;

    if (entry.url) {
      urlField.style.display = "flex";
      document.getElementById("detail-url")!.textContent = entry.url;
    } else {
      urlField.style.display = "none";
    }

    if (entry.category) {
      categoryField.style.display = "flex";
      document.getElementById("detail-category")!.textContent = entry.category;
    } else {
      categoryField.style.display = "none";
    }

    if (entry.notes) {
      notesField.style.display = "flex";
      document.getElementById("detail-notes")!.textContent = entry.notes;
    } else {
      notesField.style.display = "none";
    }

    // TOTP display
    const totpField = document.getElementById("detail-totp-field")!;
    if (entry.totp_secret) {
      totpField.style.display = "flex";
      startTotpRefresh(entry.id);
    } else {
      totpField.style.display = "none";
      stopTotpRefresh();
    }

    detailPanel.classList.remove("hidden");
  } catch (err) {
    toast("Error loading entry");
  }
}

// --- TOTP Timer ---
let totpInterval: ReturnType<typeof setInterval> | null = null;
let currentTotpEntryId: string | null = null;

function startTotpRefresh(entryId: string) {
  stopTotpRefresh();
  currentTotpEntryId = entryId;
  refreshTotpCode();
  totpInterval = setInterval(refreshTotpCode, 1000);
}

function stopTotpRefresh() {
  if (totpInterval) {
    clearInterval(totpInterval);
    totpInterval = null;
  }
  currentTotpEntryId = null;
}

async function refreshTotpCode() {
  if (!currentTotpEntryId) return;
  try {
    const result: { code: string; remaining: number } = await invoke("get_totp_code", { id: currentTotpEntryId });
    document.getElementById("detail-totp-code")!.textContent = result.code;
    document.getElementById("detail-totp-timer")!.textContent = `${result.remaining}s`;
    const fill = document.getElementById("totp-progress-fill")!;
    fill.style.width = `${(result.remaining / 30) * 100}%`;
    // Change color when running low
    if (result.remaining <= 5) {
      fill.style.background = "var(--danger)";
    } else {
      fill.style.background = "var(--accent)";
    }
  } catch {
    document.getElementById("detail-totp-code")!.textContent = "------";
    document.getElementById("detail-totp-timer")!.textContent = "";
  }
}

// Copy TOTP button
document.getElementById("copy-totp-btn")!.addEventListener("click", async () => {
  const code = document.getElementById("detail-totp-code")!.textContent || "";
  if (code && code !== "------") {
    await copyToClipboard(code);
  }
});

// Close detail
document.getElementById("close-detail")!.addEventListener("click", () => {
  detailPanel.classList.add("hidden");
  currentEntryId = null;
  currentEntryPassword = "";
  stopTotpRefresh();
  // Reset password visibility
  const pwEl = document.getElementById("detail-password")!;
  pwEl.textContent = "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022";
  pwEl.classList.add("password-masked");
  document.getElementById("toggle-pw")!.textContent = "Show";
});

// Toggle password visibility
document.getElementById("toggle-pw")!.addEventListener("click", () => {
  const pwEl = document.getElementById("detail-password")!;
  const btn = document.getElementById("toggle-pw")!;
  if (btn.textContent === "Show") {
    pwEl.textContent = currentEntryPassword;
    pwEl.classList.remove("password-masked");
    btn.textContent = "Hide";
  } else {
    pwEl.textContent = "\u2022".repeat(Math.min(currentEntryPassword.length, 20));
    pwEl.classList.add("password-masked");
    btn.textContent = "Show";
  }
});

// Copy buttons
document.querySelectorAll("[data-copy]").forEach((btn) => {
  btn.addEventListener("click", async () => {
    const field = (btn as HTMLElement).dataset.copy;
    if (field === "username") {
      const text = document.getElementById("detail-username")!.textContent || "";
      await copyToClipboard(text);
    } else if (field === "password") {
      await copyToClipboard(currentEntryPassword);
    }
  });
});

// Delete entry
document.getElementById("delete-entry-btn")!.addEventListener("click", async () => {
  if (!currentEntryId) return;
  if (!confirm("Delete this entry?")) return;
  try {
    await invoke("delete_entry", { id: currentEntryId });
    detailPanel.classList.add("hidden");
    currentEntryId = null;
    await loadEntries();
    toast("Entry deleted");
  } catch (err) {
    toast("Error deleting entry");
  }
});

// --- Check Breach ---
document.getElementById("check-breach-btn")!.addEventListener("click", async () => {
  if (!currentEntryPassword) {
    toast("No password to check");
    return;
  }
  const btn = document.getElementById("check-breach-btn")!;
  btn.textContent = "Checking...";
  btn.setAttribute("disabled", "true");
  try {
    const count: number = await invoke("check_breach", { password: currentEntryPassword });
    if (count === 0) {
      toast("Password is safe — not found in any breaches");
    } else {
      toast(`Warning: Password found in ${count.toLocaleString()} breach(es)!`);
    }
  } catch (err) {
    toast("Breach check failed: " + String(err));
  } finally {
    btn.textContent = "Check Breach";
    btn.removeAttribute("disabled");
  }
});

// --- Auto-type ---
document.getElementById("autotype-btn")!.addEventListener("click", async () => {
  if (!currentEntryId) return;
  try {
    const entry: { username: string; password: string } = await invoke("get_entry", { id: currentEntryId });
    detailPanel.classList.add("hidden");

    // Minimize the vault window — Windows will auto-focus the previous app
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.minimize();

    // Now type (the 800ms delay in Rust gives the OS time to refocus)
    const text = entry.username + "\t" + entry.password;
    await invoke("auto_type", { text, useTab: true });
  } catch (err) {
    toast("Auto-type failed: " + String(err));
  }
});

// --- Add Entry ---
document.getElementById("add-btn")!.addEventListener("click", () => {
  showScreen(addScreen);
});

document.getElementById("back-btn")!.addEventListener("click", () => {
  showScreen(vaultScreen);
});

genPwBtn.addEventListener("click", async () => {
  const password: string = await invoke("generate_password", { length: 20 });
  entryPasswordInput.value = password;
  entryPasswordInput.type = "text";
  setTimeout(() => {
    entryPasswordInput.type = "password";
  }, 3000);
});

// Scan QR for TOTP (add form)
document.getElementById("scan-qr-btn")!.addEventListener("click", async () => {
  const btn = document.getElementById("scan-qr-btn")!;
  btn.textContent = "...";
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.minimize();
    await new Promise((r) => setTimeout(r, 600));

    const result: string = await invoke("scan_qr_from_screen");
    await win.unminimize();
    await win.setFocus();

    (document.getElementById("entry-totp") as HTMLInputElement).value = result;
    toast("QR code scanned successfully");
  } catch (err) {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.unminimize();
    await win.setFocus();
    toast(String(err));
  }
  btn.textContent = "QR";
});

addForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  const name = (document.getElementById("entry-name") as HTMLInputElement).value.trim();
  const username = (document.getElementById("entry-username") as HTMLInputElement).value.trim();
  const password = entryPasswordInput.value;
  const url = (document.getElementById("entry-url") as HTMLInputElement).value.trim();
  const notes = (document.getElementById("entry-notes") as HTMLTextAreaElement).value.trim();

  if (!name) {
    toast("Name is required");
    return;
  }
  if (!password) {
    toast("Password is required");
    return;
  }

  try {
    await invoke("add_entry", {
      input: { name, username, password, url: url || null, notes: notes || null, category: (document.getElementById("entry-category") as HTMLInputElement).value.trim() || null, totp_secret: (document.getElementById("entry-totp") as HTMLInputElement).value.trim() || null },
    });
    addForm.reset();
    await loadEntries();
    showScreen(vaultScreen);
    toast("Entry saved");
  } catch (err) {
    toast("Error saving entry: " + String(err));
  }
});

// --- Lock ---
document.getElementById("lock-btn")!.addEventListener("click", async () => {
  await invoke("lock_vault");
  allEntries = [];
  entryList.innerHTML = "";
  detailPanel.classList.add("hidden");
  showScreen(authScreen);
  await initAuth();
});

// --- Export/Import ---
document.getElementById("export-btn")!.addEventListener("click", async () => {
  try {
    const selected = await open({ directory: true, title: "Choose export destination" });
    if (!selected) return;
    await invoke("export_vault", { destination: selected });
    toast("Vault exported successfully");
  } catch (err) {
    toast("Export failed: " + String(err));
  }
});

document.getElementById("import-btn")!.addEventListener("click", async () => {
  if (!confirm("Import will replace your current vault. Make sure you have the master password for the backup. Continue?")) return;
  try {
    const selected = await open({ directory: true, title: "Choose folder containing vault backup" });
    if (!selected) return;
    await invoke("import_vault", { source: selected });
    allEntries = [];
    entryList.innerHTML = "";
    detailPanel.classList.add("hidden");
    showScreen(authScreen);
    await initAuth();
    toast("Vault imported — please unlock with backup password");
  } catch (err) {
    toast("Import failed: " + String(err));
  }
});

document.getElementById("import-csv-btn")!.addEventListener("click", async () => {
  try {
    const selected = await open({
      title: "Select CSV file to import",
      filters: [{ name: "CSV Files", extensions: ["csv"] }],
    });
    if (!selected) return;
    const count: number = await invoke("import_csv", { filePath: selected });
    await loadEntries();
    toast(`Imported ${count} entries from CSV`);
  } catch (err) {
    toast("CSV import failed: " + String(err));
  }
});

document.getElementById("copy-token-btn")!.addEventListener("click", async () => {
  try {
    const code: string = await invoke("get_pairing_code");
    toast(`Pairing code: ${code}`);
  } catch {
    toast("No pairing needed — extension connects automatically");
  }
});

// --- Edit Entry ---
const editForm = document.getElementById("edit-form") as HTMLFormElement;
const editGenPwBtn = document.getElementById("edit-gen-pw-btn")!;
const editPasswordInput = document.getElementById("edit-entry-password") as HTMLInputElement;

document.getElementById("edit-entry-btn")!.addEventListener("click", async () => {
  if (!currentEntryId) return;
  try {
    const entry: {
      id: string;
      name: string;
      username: string;
      password: string;
      url: string;
      notes: string;
      category: string;
      created_at: string;
      totp_secret: string;
    } = await invoke("get_entry", { id: currentEntryId });

    (document.getElementById("edit-entry-id") as HTMLInputElement).value = entry.id;
    (document.getElementById("edit-entry-name") as HTMLInputElement).value = entry.name;
    (document.getElementById("edit-entry-username") as HTMLInputElement).value = entry.username;
    editPasswordInput.value = "";
    (document.getElementById("edit-entry-url") as HTMLInputElement).value = entry.url;
    (document.getElementById("edit-entry-category") as HTMLInputElement).value = entry.category;
    (document.getElementById("edit-entry-notes") as HTMLTextAreaElement).value = entry.notes;
    (document.getElementById("edit-entry-totp") as HTMLInputElement).value = entry.totp_secret || "";

    detailPanel.classList.add("hidden");
    showScreen(editScreen);
  } catch (err) {
    toast("Error loading entry for edit");
  }
});

document.getElementById("edit-back-btn")!.addEventListener("click", () => {
  showScreen(vaultScreen);
});

editGenPwBtn.addEventListener("click", async () => {
  const password: string = await invoke("generate_password", { length: 20 });
  editPasswordInput.value = password;
  editPasswordInput.type = "text";
  setTimeout(() => {
    editPasswordInput.type = "password";
  }, 3000);
});

// Scan QR for TOTP (edit form)
document.getElementById("edit-scan-qr-btn")!.addEventListener("click", async () => {
  const btn = document.getElementById("edit-scan-qr-btn")!;
  btn.textContent = "...";
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.minimize();
    await new Promise((r) => setTimeout(r, 600));

    const result: string = await invoke("scan_qr_from_screen");
    await win.unminimize();
    await win.setFocus();

    (document.getElementById("edit-entry-totp") as HTMLInputElement).value = result;
    toast("QR code scanned successfully");
  } catch (err) {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.unminimize();
    await win.setFocus();
    toast(String(err));
  }
  btn.textContent = "QR";
});

editForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  const id = (document.getElementById("edit-entry-id") as HTMLInputElement).value;
  const name = (document.getElementById("edit-entry-name") as HTMLInputElement).value.trim();
  const username = (document.getElementById("edit-entry-username") as HTMLInputElement).value.trim();
  const password = editPasswordInput.value;
  const url = (document.getElementById("edit-entry-url") as HTMLInputElement).value.trim();
  const category = (document.getElementById("edit-entry-category") as HTMLInputElement).value.trim();
  const notes = (document.getElementById("edit-entry-notes") as HTMLTextAreaElement).value.trim();

  if (!name) {
    toast("Name is required");
    return;
  }

  const totp_secret = (document.getElementById("edit-entry-totp") as HTMLInputElement).value.trim();

  const input: Record<string, string | null> = {
    id,
    name,
    username,
    password: password || null,
    url,
    category,
    notes,
    totp_secret: totp_secret || null,
  };

  try {
    await invoke("edit_entry", { input });
    editForm.reset();
    await loadEntries();
    showScreen(vaultScreen);
    toast("Entry updated");
  } catch (err) {
    toast("Error updating entry: " + String(err));
  }
});

// --- Utility ---
function escapeHtml(text: string): string {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

// --- Password Strength Meter ---
const pwStrengthBar = document.getElementById("pw-strength-bar")!;
const pwStrengthFill = document.getElementById("pw-strength-fill")!;
const pwStrengthLabel = document.getElementById("pw-strength-label")!;

function evaluateStrength(password: string): { score: number; label: string; color: string } {
  let score = 0;
  if (password.length >= 8) score++;
  if (password.length >= 12) score++;
  if (password.length >= 16) score++;
  if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score++;
  if (/\d/.test(password)) score++;
  if (/[^a-zA-Z0-9]/.test(password)) score++;

  if (score <= 2) return { score, label: "Weak", color: "#e94560" };
  if (score <= 3) return { score, label: "Fair", color: "#f5a623" };
  if (score <= 4) return { score, label: "Good", color: "#4ecdc4" };
  return { score, label: "Strong", color: "#2ecc71" };
}

masterPasswordInput.addEventListener("input", () => {
  const pw = masterPasswordInput.value;
  // Only show strength meter during vault creation
  if (confirmPasswordInput.style.display === "none") {
    pwStrengthBar.style.display = "none";
    pwStrengthLabel.style.display = "none";
    return;
  }
  if (pw.length === 0) {
    pwStrengthBar.style.display = "none";
    pwStrengthLabel.style.display = "none";
    return;
  }
  pwStrengthBar.style.display = "block";
  pwStrengthLabel.style.display = "block";
  const { score, label, color } = evaluateStrength(pw);
  const percent = Math.min((score / 6) * 100, 100);
  pwStrengthFill.style.width = `${percent}%`;
  pwStrengthFill.style.background = color;
  pwStrengthLabel.textContent = label;
  pwStrengthLabel.style.color = color;
});

// --- Auto-lock (idle timer) ---
const IDLE_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
let idleTimer: ReturnType<typeof setTimeout> | null = null;

function resetIdleTimer() {
  if (idleTimer) clearTimeout(idleTimer);
  idleTimer = setTimeout(async () => {
    // Only lock if vault is currently unlocked
    const unlocked: boolean = await invoke("is_unlocked");
    if (unlocked) {
      await invoke("lock_vault");
      allEntries = [];
      entryList.innerHTML = "";
      detailPanel.classList.add("hidden");
      showScreen(authScreen);
      await initAuth();
      toast("Vault locked due to inactivity");
    }
  }, IDLE_TIMEOUT_MS);
}

["mousemove", "mousedown", "keydown", "touchstart", "scroll", "click"].forEach((event) => {
  document.addEventListener(event, resetIdleTimer, { passive: true });
});

// --- Init ---
initAuth();
resetIdleTimer();

// --- Pairing code display (poll for rate-limit lockout) ---
let lastPairingCode = "";
setInterval(async () => {
  try {
    const code: string = await invoke("get_pairing_code");
    if (code && code !== lastPairingCode) {
      lastPairingCode = code;
      showPairingBanner(code);
      // Also fetch exposed entries for breach alert
      fetchExposedEntries();
    }
  } catch {
    // No pairing code active — dismiss banner if shown
    if (lastPairingCode) {
      lastPairingCode = "";
      dismissPairingBanner();
    }
  }
}, 2000);

async function fetchExposedEntries() {
  try {
    const res = await fetch("http://127.0.0.1:7890/exposed-entries");
    if (res.ok) {
      const entries: Array<{ name: string; username: string; url: string }> = await res.json();
      if (entries.length > 0) {
        showBreachAlert(entries);
      }
    }
  } catch {
    // Ignore — server might not be ready
  }
}

function showBreachAlert(entries: Array<{ name: string; username: string; url: string }>) {
  let panel = document.getElementById("breach-alert");
  if (!panel) {
    panel = document.createElement("div");
    panel.id = "breach-alert";
    panel.style.cssText = `
      position: fixed; top: 40px; left: 50%; transform: translateX(-50%);
      background: #1a1a2e; border: 2px solid #e94560; border-radius: 12px;
      padding: 20px; max-width: 360px; width: 90%; z-index: 201;
      box-shadow: 0 8px 32px rgba(233,69,96,0.3); font-family: -apple-system, sans-serif;
    `;
    document.body.appendChild(panel);
  }

  const entryList = entries.map(e =>
    `<div style="display:flex;align-items:center;gap:8px;padding:6px 0;border-bottom:1px solid #2a2a4a;">
      <span style="color:#e94560;font-size:16px;">&#x26A0;</span>
      <div style="flex:1;min-width:0;">
        <div style="font-size:13px;color:#eaeaea;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${escapeHtml(e.name)}</div>
        <div style="font-size:11px;color:#a0a0b0;">${escapeHtml(e.username)}</div>
      </div>
    </div>`
  ).join("");

  panel.innerHTML = `
    <div style="display:flex;align-items:center;gap:10px;margin-bottom:12px;">
      <span style="font-size:28px;color:#e94560;">&#x26A0;</span>
      <div>
        <div style="font-size:15px;font-weight:600;color:#e94560;">Security Alert</div>
        <div style="font-size:12px;color:#a0a0b0;">Suspicious API activity detected</div>
      </div>
    </div>
    <div style="font-size:12px;color:#eaeaea;margin-bottom:10px;line-height:1.5;">
      The following credentials were accessed before the lockout triggered. They may have been compromised:
    </div>
    <div style="max-height:150px;overflow-y:auto;margin-bottom:12px;">
      ${entryList}
    </div>
    <div style="background:#2a1520;border:1px solid #4a2639;border-radius:8px;padding:10px;margin-bottom:14px;">
      <div style="font-size:12px;color:#f472b6;font-weight:500;margin-bottom:4px;">&#x1F6A8; Recommendation</div>
      <div style="font-size:11px;color:#a0a0b0;line-height:1.4;">
        Change <strong style="color:#eaeaea;">all</strong> your passwords, not just the ones listed above. If an attacker had local access, they may have captured more than what's shown here.
      </div>
    </div>
    <button id="dismiss-breach-alert" style="
      width:100%;background:#2a2a4a;border:1px solid #4a4a6a;border-radius:6px;
      padding:10px;color:#a0a0b0;font-size:12px;cursor:pointer;
    ">Dismiss</button>
  `;

  document.getElementById("dismiss-breach-alert")!.addEventListener("click", async () => {
    try {
      await fetch("http://127.0.0.1:7890/dismiss-alert", { method: "POST" });
    } catch {}
    panel!.remove();
  });
}

function showPairingBanner(code: string) {
  let banner = document.getElementById("pairing-banner");
  if (!banner) {
    banner = document.createElement("div");
    banner.id = "pairing-banner";
    banner.style.cssText = `
      position: fixed; top: 0; left: 0; right: 0;
      background: #f5a623; color: #111; padding: 12px 16px;
      font-size: 14px; font-weight: 600; text-align: center;
      z-index: 200; font-family: monospace;
    `;
    document.body.appendChild(banner);
  }
  banner.innerHTML = `Extension pairing code: <span style="font-size:20px;letter-spacing:4px;margin-left:8px;">${code}</span>`;
}

function dismissPairingBanner() {
  const banner = document.getElementById("pairing-banner");
  if (banner) banner.remove();
}

// --- Start on boot toggle ---
const bootToggle = document.getElementById("start-on-boot-toggle") as HTMLInputElement;
(async () => {
  try {
    const enabled: boolean = await invoke("get_start_on_boot");
    bootToggle.checked = enabled;
  } catch (e) {}
})();
bootToggle.addEventListener("change", async () => {
  try {
    await invoke("set_start_on_boot", { enabled: bootToggle.checked });
    toast(bootToggle.checked ? "Will start on boot" : "Won't start on boot");
  } catch (err) {
    toast("Failed: " + String(err));
    bootToggle.checked = !bootToggle.checked;
  }
});

// --- Sync ---
const syncFolderPath = document.getElementById("sync-folder-path")!;

(async () => {
  try {
    const folder: string | null = await invoke("get_sync_folder");
    if (folder) {
      syncFolderPath.textContent = folder;
    }
  } catch (e) {}
})();

document.getElementById("sync-folder-btn")!.addEventListener("click", async () => {
  try {
    const selected = await open({ directory: true, title: "Choose sync folder (e.g. OneDrive, Google Drive)" });
    if (!selected) return;
    await invoke("set_sync_folder", { folder: selected });
    syncFolderPath.textContent = selected as string;
    toast("Sync folder set");
  } catch (err) {
    toast("Failed: " + String(err));
  }
});

document.getElementById("sync-push-btn")!.addEventListener("click", async () => {
  try {
    const msg: string = await invoke("sync_push");
    toast(msg);
  } catch (err) {
    toast("Push failed: " + String(err));
  }
});

document.getElementById("sync-pull-btn")!.addEventListener("click", async () => {
  if (!confirm("Pull will replace your local vault with the remote copy. Continue?")) return;
  try {
    const msg: string = await invoke("sync_pull");
    toast(msg);
    showScreen(authScreen);
    await initAuth();
  } catch (err) {
    toast("Pull failed: " + String(err));
  }
});

// Refresh entries when window regains focus (picks up entries added by browser extension)
window.addEventListener("focus", async () => {
  const unlocked: boolean = await invoke("is_unlocked");
  if (unlocked) {
    await loadEntries();
  }
});

// --- Theme System ---
let currentTheme: ThemeConfig = loadSavedTheme();
applyTheme(currentTheme);

function renderThemePresets() {
  const container = document.getElementById("theme-presets")!;
  container.innerHTML = PRESETS.map((preset) => {
    const isActive = currentTheme.id === preset.id;
    return `
      <div class="theme-preset-card${isActive ? " active" : ""}" data-theme-id="${preset.id}">
        <div class="preview">
          <div class="preview-swatch" style="background:${preset.colors.bg}"></div>
          <div class="preview-swatch" style="background:${preset.colors.bgCard}"></div>
          <div class="preview-swatch" style="background:${preset.colors.accent}"></div>
          <div class="preview-swatch" style="background:${preset.colors.text}"></div>
        </div>
        <span class="preset-name">${preset.name}</span>
      </div>`;
  }).join("");

  container.querySelectorAll(".theme-preset-card").forEach((card) => {
    card.addEventListener("click", () => {
      const id = (card as HTMLElement).dataset.themeId!;
      const preset = PRESETS.find((p) => p.id === id);
      if (preset) {
        currentTheme = { ...preset };
        applyTheme(currentTheme);
        syncColorInputs();
        renderThemePresets();
      }
    });
  });
}

function syncColorInputs() {
  const fields: Array<keyof ThemeColors> = [
    "bg", "bgCard", "bgInput", "accent", "accentHover",
    "text", "textDim", "border", "success", "danger",
  ];
  fields.forEach((key) => {
    const input = document.getElementById(`color-${key}`) as HTMLInputElement | null;
    if (input) {
      input.value = currentTheme.colors[key];
    }
  });
}

function initThemeScreen() {
  renderThemePresets();
  syncColorInputs();

  // Color picker change handlers
  document.querySelectorAll("#theme-custom input[type='color']").forEach((input) => {
    input.addEventListener("input", (e) => {
      const el = e.target as HTMLInputElement;
      const varName = el.dataset.var as keyof ThemeColors;
      if (varName) {
        currentTheme = createCustomTheme(currentTheme, { [varName]: el.value });
        applyTheme(currentTheme);
        renderThemePresets(); // deselect presets since it's now custom
      }
    });
  });
}

// Theme button opens theme screen
document.getElementById("theme-btn")!.addEventListener("click", () => {
  initThemeScreen();
  showScreen(themeScreen);
});

// Back button
document.getElementById("theme-back-btn")!.addEventListener("click", () => {
  showScreen(vaultScreen);
});

// Save button
document.getElementById("theme-save-btn")!.addEventListener("click", () => {
  saveTheme(currentTheme);
  toast("Theme saved");
  showScreen(vaultScreen);
});

// Reset button
document.getElementById("theme-reset-btn")!.addEventListener("click", () => {
  currentTheme = { ...PRESETS[0] };
  applyTheme(currentTheme);
  saveTheme(currentTheme);
  syncColorInputs();
  renderThemePresets();
  toast("Theme reset to default");
});
