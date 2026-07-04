const API_BASE = "http://127.0.0.1:7890";

const statusEl = document.getElementById("status");
const contentEl = document.getElementById("content");
const pairingSection = document.getElementById("pairing-section");
const pairCodeInput = document.getElementById("pair-code-input");
const pairBtn = document.getElementById("pair-btn");
const pairError = document.getElementById("pair-error");

// Auto-connect on open (no token needed)
checkStatusAndLoad();

pairBtn.addEventListener("click", submitPairingCode);
pairCodeInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") submitPairingCode();
});

async function submitPairingCode() {
  const code = pairCodeInput.value.trim();
  if (code.length !== 6) {
    pairError.textContent = "Enter the 6-digit code";
    return;
  }

  pairBtn.textContent = "Pairing...";
  pairBtn.disabled = true;
  pairError.textContent = "";

  try {
    const res = await fetch(`${API_BASE}/pair`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code }),
    });

    if (res.ok) {
      pairingSection.classList.remove("active");
      pairBtn.textContent = "Pair";
      pairBtn.disabled = false;
      pairCodeInput.value = "";
      await checkStatusAndLoad();
    } else {
      pairError.textContent = "Wrong code. Check Vault app.";
      pairBtn.textContent = "Pair";
      pairBtn.disabled = false;
    }
  } catch (err) {
    pairError.textContent = "Cannot reach Vault";
    pairBtn.textContent = "Pair";
    pairBtn.disabled = false;
  }
}

async function checkStatusAndLoad() {
  try {
    const res = await fetch(`${API_BASE}/status`);
    const data = await res.json();

    if (data.unlocked) {
      statusEl.textContent = "Connected";
      statusEl.className = "status unlocked";
      pairingSection.classList.remove("active");
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
    const res = await fetch(`${API_BASE}/entries?domain=${encodeURIComponent(domain)}`);

    if (res.status === 429) {
      // Rate limited — show pairing UI
      statusEl.textContent = "Pairing Required";
      statusEl.className = "status pairing";
      pairingSection.classList.add("active");
      contentEl.innerHTML = "";
      return;
    }

    if (res.status === 403) {
      contentEl.innerHTML = '<p class="empty">Vault is locked. Unlock it in the desktop app.</p>';
      return;
    }

    const entries = await res.json();

    if (entries.length === 0) {
      contentEl.innerHTML = `<p class="empty">No entries for ${escapeHtml(domain) || "this site"}</p>`;
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
