const ENDPOINT = "http://127.0.0.1:47653/media";
const SOURCE = "sound-connect-chromium";
const states = new Map();
const reportQueues = new Map();

const MUSIC_HOSTS = new Set([
  "music.youtube.com",
  "music.apple.com",
  "tidal.com"
]);

const MOVIE_HOSTS = new Set([
  "netflix.com",
  "disneyplus.com",
  "max.com",
  "hulu.com",
  "hbomax.com",
  "primevideo.com",
  "tv.apple.com",
  "paramountplus.com",
  "peacocktv.com",
  "jiohotstar.com",
  "hotstar.com"
]);

function matchesHost(hostname, base) {
  return hostname === base || hostname.endsWith(`.${base}`);
}

function classify(hostname) {
  if ([...MUSIC_HOSTS].some((base) => matchesHost(hostname, base))) return "music";
  if ([...MOVIE_HOSTS].some((base) => matchesHost(hostname, base))) return "movie";
  return "other";
}

function hostnameFromUrl(url) {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:" ? parsed.hostname : "";
  } catch {
    return "";
  }
}

async function getToken() {
  const result = await chrome.storage.local.get("pairingToken");
  return typeof result.pairingToken === "string" ? result.pairingToken.trim() : "";
}

async function getSessionId(tabId) {
  const key = `session:${tabId}`;
  const saved = await chrome.storage.session.get(key);
  if (typeof saved[key] === "string") return saved[key];
  const sessionId = `chromium-${tabId}-${crypto.randomUUID()}`;
  await chrome.storage.session.set({ [key]: sessionId });
  return sessionId;
}

async function postState(tabId, url, playing) {
  const hostname = hostnameFromUrl(url);
  if (!hostname) return;
  const token = await getToken();
  if (!token) return;
  const sessionId = await getSessionId(tabId);
  const payload = {
    token,
    source: SOURCE,
    site: hostname,
    kind: classify(hostname),
    playing: Boolean(playing),
    spatial: false,
    sessionId
  };
  try {
    const response = await fetch(ENDPOINT, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      cache: "no-store",
      signal: AbortSignal.timeout(1500)
    });
    if (!response.ok) chrome.runtime.sendMessage({ type: "media-report-error", status: response.status });
  } catch {
    // The local desktop endpoint may be closed. The next media event or heartbeat retries.
  }
}

async function reportTab(tabId, url, playing = false) {
  const previous = states.get(tabId);
  // Do not report ordinary browsing or inactive tabs. A stop is needed only
  // when this worker previously reported playback from this tab.
  if (!playing && !previous?.playing) return;
  const state = previous ?? { url, playing: false };
  state.url = url || state.url;
  state.playing = Boolean(playing);
  states.set(tabId, state);
  await sendQueued(tabId, state.url, state.playing);
}

async function sendQueued(tabId, url, playing) {
  // Serialize transitions and session-ID creation per tab, so a delayed play
  // request cannot arrive after pause or create a second ID for the same tab.
  const task = (reportQueues.get(tabId) ?? Promise.resolve()).catch(() => {}).then(() => postState(tabId, url, playing));
  reportQueues.set(tabId, task);
  await task;
  if (reportQueues.get(tabId) === task) reportQueues.delete(tabId);
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message?.type === "pairing-saved" && sender.id === chrome.runtime.id && sender.url === chrome.runtime.getURL("popup.html")) {
    void injectIntoExistingTabs().then(sendResponse);
    return true;
  }
  if (sender.frameId !== 0 || !sender.tab || !Number.isInteger(sender.tab.id)) return;
  if (!message || message.type !== "media-state" || typeof message.playing !== "boolean") return;
  const url = sender.tab.url || sender.url;
  if (!url) return;
  void reportTab(sender.tab.id, url, message.playing);
});

async function injectIntoExistingTabs() {
  const tabs = await chrome.tabs.query({});
  let injected = 0;
  let skipped = 0;
  for (const tab of tabs) {
    if (!Number.isInteger(tab.id) || !/^https?:\/\//i.test(tab.url || "")) continue;
    try {
      await chrome.scripting.executeScript({ target: { tabId: tab.id }, files: ["content.js"] });
      injected += 1;
    } catch {
      // Browser-owned pages or withheld site access cannot accept content scripts.
      skipped += 1;
    }
  }
  return { injected, skipped };
}

chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.url || changeInfo.status === "loading" || changeInfo.status === "complete") {
    const url = changeInfo.url || tab.url;
    const existing = states.get(tabId);
    const playing = changeInfo.status === "complete" && existing?.url === url && existing.playing;
    if (url) void reportTab(tabId, url, Boolean(playing));
  }
});

chrome.tabs.onRemoved.addListener(async (tabId) => {
  const state = states.get(tabId);
  if (state?.playing && state.url) await sendQueued(tabId, state.url, false);
  states.delete(tabId);
  await chrome.storage.session.remove(`session:${tabId}`);
});

chrome.runtime.onInstalled.addListener(async () => {
  const tabs = await chrome.tabs.query({});
  for (const tab of tabs) {
    if (Number.isInteger(tab.id) && tab.url) void reportTab(tab.id, tab.url, false);
  }
});

chrome.tabs.onActivated.addListener(async ({ tabId }) => {
  try {
    const tab = await chrome.tabs.get(tabId);
    if (tab.url) void reportTab(tabId, tab.url, states.get(tabId)?.playing ?? false);
  } catch {
    // The tab can close between activation and lookup.
  }
});
