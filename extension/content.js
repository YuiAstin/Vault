// Content script: fills login forms and detects form submissions

const API_BASE = "http://127.0.0.1:7890";

// Listen for fill commands from popup
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "VAULT_FILL") {
    fillForm(message.username, message.password);
    sendResponse({ success: true });
  }
  return true;
});

function fillForm(username, password) {
  const usernameField = findUsernameField();
  const passwordField = findPasswordField();

  if (usernameField) setNativeValue(usernameField, username);
  if (passwordField) setNativeValue(passwordField, password);
}

function findUsernameField() {
  const selectors = [
    'input[autocomplete="username"]',
    'input[autocomplete="email"]',
    'input[name="username"]',
    'input[name="email"]',
    'input[name="login"]',
    'input[name="user"]',
    'input[id="username"]',
    'input[id="email"]',
    'input[id="login"]',
    'input[type="email"]',
    'input[name*="user"]',
    'input[name*="email"]',
    'input[name*="login"]',
  ];

  for (const selector of selectors) {
    const el = document.querySelector(selector);
    if (el && isVisible(el)) return el;
  }

  // Fallback: find text/email input near a password field
  const passwordField = findPasswordField();
  if (passwordField) {
    const form = passwordField.closest("form");
    if (form) {
      const inputs = form.querySelectorAll('input[type="text"], input[type="email"], input:not([type])');
      for (const input of inputs) {
        if (isVisible(input)) return input;
      }
    }
  }
  return null;
}

function findPasswordField() {
  const selectors = [
    'input[autocomplete="current-password"]',
    'input[autocomplete="new-password"]',
    'input[type="password"]',
  ];
  for (const selector of selectors) {
    const el = document.querySelector(selector);
    if (el && isVisible(el)) return el;
  }
  return null;
}

function isVisible(el) {
  const style = window.getComputedStyle(el);
  return (
    style.display !== "none" &&
    style.visibility !== "hidden" &&
    style.opacity !== "0" &&
    el.offsetWidth > 0 &&
    el.offsetHeight > 0
  );
}

function setNativeValue(el, value) {
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  setter.call(el, value);
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

// --- Auto-save detection ---
let lastPromptKey = "";

function detectFormSubmit() {
  // 1. Native form submit
  document.addEventListener("submit", (e) => {
    const form = e.target;
    if (!(form instanceof HTMLFormElement)) return;
    const creds = extractCredentials(form);
    if (!creds) return;

    e.preventDefault();
    e.stopPropagation();
    showSavePrompt(creds.username, creds.password, () => form.submit());
  }, true);

  // 2. Click on submit buttons
  document.addEventListener("click", (e) => {
    const target = e.target.closest('button[type="submit"], input[type="submit"]');
    if (!target) return;

    const form = target.closest("form");
    if (!form) return;

    const creds = extractCredentials(form);
    if (!creds) return;

    e.preventDefault();
    e.stopPropagation();
    showSavePrompt(creds.username, creds.password, () => form.submit());
  }, true);

  // 3. Enter on password field
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    if (!(e.target instanceof HTMLInputElement) || e.target.type !== "password") return;
    if (!e.target.value) return;

    const form = e.target.closest("form");
    if (!form) return;

    const creds = extractCredentials(form);
    if (!creds) return;

    e.preventDefault();
    e.stopPropagation();
    showSavePrompt(creds.username, creds.password, () => form.submit());
  }, true);
}

function extractCredentials(form) {
  const passwordField = form.querySelector('input[type="password"]');
  if (!passwordField || !passwordField.value) return null;

  const inputs = form.querySelectorAll('input[type="text"], input[type="email"], input:not([type])');
  let username = "";
  for (const input of inputs) {
    if (input.value && isVisible(input) && input !== passwordField) {
      username = input.value;
      break;
    }
  }

  return { username, password: passwordField.value };
}

function showSavePrompt(username, password, onDone) {
  const key = `${username}:${password}`;
  if (key === lastPromptKey) {
    if (onDone) onDone();
    return;
  }
  lastPromptKey = key;

  // Check if this credential already exists before showing the prompt
  (async () => {
    try {
      const domain = window.location.hostname.replace("www.", "");
      const res = await fetch(`${API_BASE}/entries?domain=${encodeURIComponent(domain)}`);
      if (res.ok) {
        const entries = await res.json();
        const isDuplicate = entries.some(e =>
          e.username.toLowerCase() === username.toLowerCase()
        );
        if (isDuplicate) {
          if (onDone) onDone();
          return;
        }
      }
      // If 429 (rate limited), just show prompt anyway — saving won't work but that's fine
    } catch (e) {
      // Can't reach vault, show prompt anyway
    }

    showSavePromptUI(username, password, onDone);
  })();
}

function showSavePromptUI(username, password, onDone) {
  const existing = document.getElementById("vault-save-prompt");
  if (existing) existing.remove();

  const banner = document.createElement("div");
  banner.id = "vault-save-prompt";
  banner.innerHTML = `
    <div style="position:fixed;top:12px;right:12px;z-index:2147483647;background:#1a1a2e;border:1px solid #2a2a4a;border-radius:10px;padding:14px 18px;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;color:#eaeaea;box-shadow:0 8px 32px rgba(0,0,0,0.4);max-width:340px;font-size:13px;line-height:1.4;">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:10px;">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#e94560" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        <strong style="font-size:14px;">Save to Vault?</strong>
      </div>
      <div style="color:#a0a0b0;margin-bottom:12px;">
        <div>${escapeHtml(window.location.hostname)}</div>
        <div style="color:#eaeaea;">${escapeHtml(username || "(no username)")}</div>
      </div>
      <div style="display:flex;gap:8px;">
        <button id="vault-save-yes" style="flex:1;background:#e94560;border:none;border-radius:6px;padding:8px 12px;color:white;font-size:12px;font-weight:500;cursor:pointer;">Save</button>
        <button id="vault-save-no" style="flex:1;background:transparent;border:1px solid #2a2a4a;border-radius:6px;padding:8px 12px;color:#a0a0b0;font-size:12px;cursor:pointer;">Skip</button>
      </div>
    </div>
  `;
  document.body.appendChild(banner);

  document.getElementById("vault-save-yes").addEventListener("click", async () => {
    const btn = document.getElementById("vault-save-yes");
    btn.textContent = "Saving...";
    btn.disabled = true;

    try {
      const res = await fetch(`${API_BASE}/entries`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: username,
          password: password,
          url: window.location.origin,
          name: window.location.hostname.replace("www.", ""),
        }),
      });

      if (res.status === 429) {
        btn.textContent = "Rate limited";
        btn.style.background = "#f5a623";
        setTimeout(() => { banner.remove(); if (onDone) onDone(); }, 2000);
        return;
      }

      if (res.ok) {
        const data = await res.json().catch(() => ({}));
        if (data.reason === "duplicate") {
          btn.textContent = "Already saved";
          btn.style.background = "#a0a0b0";
        } else {
          btn.textContent = "Saved!";
          btn.style.background = "#4ecdc4";
        }
        setTimeout(() => { banner.remove(); if (onDone) onDone(); }, 1000);
      } else {
        const err = await res.json().catch(() => ({ error: "Unknown error" }));
        btn.textContent = err.error || "Failed";
        setTimeout(() => { banner.remove(); if (onDone) onDone(); }, 2500);
      }
    } catch (e) {
      btn.textContent = "Vault offline";
      setTimeout(() => { banner.remove(); if (onDone) onDone(); }, 2500);
    }
  });

  document.getElementById("vault-save-no").addEventListener("click", () => {
    banner.remove();
    if (onDone) onDone();
  });
}

function escapeHtml(text) {
  const d = document.createElement("div");
  d.textContent = text;
  return d.innerHTML;
}

detectFormSubmit();
