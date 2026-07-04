// Background service worker
// Handles notifications about new credentials detected on form submit
// Compatible with both Chrome and Firefox

let pendingCredential = null;

console.log("[Vault] Background service worker started");

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log("[Vault] Received message:", message.type, message.domain, message.username);

  if (message.type === "VAULT_NEW_CREDENTIALS") {
    pendingCredential = message;

    // Show notification offering to save
    chrome.notifications.create("vault-save-prompt", {
      type: "basic",
      iconUrl: "icons/icon48.png",
      title: "Save to Vault?",
      message: `Login detected for ${message.domain} (${message.username})`,
    }, (notifId) => {
      if (chrome.runtime.lastError) {
        console.error("[Vault] Notification error:", chrome.runtime.lastError.message);
      } else {
        console.log("[Vault] Notification shown:", notifId);
      }
    });

    sendResponse({ received: true });
  }
  return true;
});

// Handle notification click (works in both Chrome and Firefox)
chrome.notifications.onClicked.addListener((notifId) => {
  console.log("[Vault] Notification clicked:", notifId);
  if (notifId === "vault-save-prompt" && pendingCredential) {
    // Future: POST to vault API to save the entry
    pendingCredential = null;
    chrome.notifications.clear(notifId);
  }
});
