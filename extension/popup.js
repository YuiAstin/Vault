const API_BASE = "http://127.0.0.1:7890";

const statusEl = document.getElementById("status");
const contentEl = document.getElementById("content");
const tokenSection = document.getElementById("token-section");
const tokenInput = document.getElementById("token-input");
const saveTokenBtn = document.getElementById("save-token");

let apiToken = "";

// Load saved token
chrome.storage.local.get(["vaultToken"], (result) => {
  if (result.vaultToken) {
    apiToken = result.vaultToken;
    tokenInput.value = apiToken;
    checkStatusAndLoad();
  }
});

saveTokenBtn.addEventListener("click", () => {
  apiToken = tokenInput.value.trim();
  if (!apiToken) return;
  chrome.storage.local.set({ vaultToken: apiToken }, () => {
    checkStatusAndLoad();
  });
});

async function checkStatusAndLoad() {
  try {
    const res = await fetch(`${API_BASE}/status`);
    const data = await res.json();

    if (data.unlocked) {
      statusEl.textContent = "Unlocked";
      statusEl.className = "status unlocked";
      tokenSection.style.display = "none";
      await loadEntries();
    } else {
      statusEl.textContent = "Locked";
      statusEl.className = "status locked";
      contentEl.innerHTML = '<p class="empty">Vault is locked. Unlock it in the desktop app.</p>';
    }
  } catch (err) {
    statusEl.textContent = "Offline";
    statusEl.className = "status locked";
    contentEl.innerHTML = '<p class="error">Cannot connect to Vault app. Is it running?</p>';
  }
}

async function loadEntries() {
  // Get current tab domain
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  let domain = "";
  try {
    const url = new URL(tab.url);
    domain = url.hostname.replace("www.", "");
  } catch (e) {}

  try {
    const res = await fetch(`${API_BASE}/entries?domain=${encodeURIComponent(domain)}`, {
      headers: { Authorization: `Bearer ${apiToken}` },
    });

    if (res.status === 401) {
      contentEl.innerHTML = '<p class="error">Invalid token. Update it above.</p>';
      tokenSection.style.display = "block";
      return;
    }

    const entries = await res.json();

    if (entries.length === 0) {
      contentEl.innerHTML = `<p class="empty">No entries for ${domain || "this site"}</p>`;
      return;
    }

    contentEl.innerHTML = '<div class="entries">' +
      entries.map((e) => `
        <div class="entry-item" data-username="${escapeAttr(e.username)}" data-password="${escapeAttr(e.password)}">
          <div class="entry-icon">${e.name.charAt(0).toUpperCase()}</div>
          <div class="entry-info">
            <div class="name">${escapeHtml(e.name)}</div>
            <div class="username">${escapeHtml(e.username)}</div>
          </div>
        </div>
      `).join("") +
      "</div>";

    // Click to fill
    document.querySelectorAll(".entry-item").forEach((el) => {
      el.addEventListener("click", () => {
        const username = el.dataset.username;
        const password = el.dataset.password;
        // Send to content script
        chrome.tabs.sendMessage(tab.id, {
          type: "VAULT_FILL",
          username,
          password,
        });
        window.close();
      });
    });
  } catch (err) {
    contentEl.innerHTML = '<p class="error">Failed to load entries</p>';
  }
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function escapeAttr(text) {
  return text.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
