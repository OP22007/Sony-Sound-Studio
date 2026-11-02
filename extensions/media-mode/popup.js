const form = document.getElementById("pair-form");
const tokenInput = document.getElementById("token");
const status = document.getElementById("status");

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === "media-report-error") {
    status.textContent = `Desktop endpoint rejected a report (HTTP ${message.status}). Check that Follow media is enabled.`;
  }
});

chrome.storage.local.get("pairingToken").then(({ pairingToken }) => {
  if (typeof pairingToken === "string" && pairingToken) {
    tokenInput.value = pairingToken;
    status.textContent = "Checking saved pairing key…";
    form.requestSubmit();
  }
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const pairingToken = tokenInput.value.trim();
  if (!pairingToken) return;
  status.textContent = "Connecting to the desktop app…";
  let response;
  try {
    response = await fetch("http://127.0.0.1:47653/pair", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ token: pairingToken }),
      cache: "no-store",
      signal: AbortSignal.timeout(5000)
    });
  } catch {
    status.textContent = "Could not reach the desktop app. Check that it is running and try again.";
    return;
  }
  if (!response.ok) {
    status.textContent = `Pairing failed (HTTP ${response.status}). Check the token and that Follow media is enabled.`;
    return;
  }
  let result;
  try {
    result = await response.json();
  } catch {
    status.textContent = "The desktop app returned an invalid pairing response.";
    return;
  }
  if (result?.ok !== true) {
    status.textContent = "The desktop app did not confirm pairing.";
    return;
  }
  try {
    await chrome.storage.local.set({ pairingToken });
    status.textContent = "Connected. Checking open tabs for media…";
    const tabs = await chrome.runtime.sendMessage({ type: "pairing-saved" });
    status.textContent = tabs?.skipped
      ? `Connected. ${tabs.injected} open tabs are ready; reload ${tabs.skipped} tabs the browser could not access.`
      : `Connected. ${tabs?.injected ?? 0} open tabs are ready.`;
  } catch {
    status.textContent = "The desktop confirmed pairing, but this browser could not prepare open tabs. Reload the active tab.";
  }
});
