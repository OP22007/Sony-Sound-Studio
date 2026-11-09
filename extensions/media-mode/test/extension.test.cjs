const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const root = path.resolve(__dirname, "..");

test("classifies only the explicit streaming host allowlist", () => {
  const source = fs.readFileSync(path.join(root, "background.js"), "utf8");
  const context = { chrome: stubChrome(), fetch: async () => ({ ok: true }), URL, crypto: { randomUUID: () => "nonce" }, AbortSignal };
  vm.runInNewContext(source, context);
  assert.equal(vm.runInNewContext('classify("www.netflix.com")', context), "movie");
  assert.equal(vm.runInNewContext('classify("jiohotstar.com")', context), "movie");
  assert.equal(vm.runInNewContext('classify("music.youtube.com")', context), "music");
  assert.equal(vm.runInNewContext('classify("open.spotify.com")', context), "music");
  assert.equal(vm.runInNewContext('classify("youtube.com")', context), "other");
  assert.equal(vm.runInNewContext('classify("video.example")', context), "other");
});

test("background sends the native JSON contract and trusts the sender tab URL", async () => {
  const requests = [];
  const chrome = stubChrome();
  const context = {
    chrome,
    fetch: async (url, options) => { requests.push({ url, options }); return { ok: true }; },
    URL,
    crypto: { randomUUID: () => "stable-nonce" },
    AbortSignal
  };
  vm.runInNewContext(fs.readFileSync(path.join(root, "background.js"), "utf8"), context);
  const listener = chrome.runtime.messageListeners[0];
  listener({ type: "media-state", playing: true, url: "https://attacker.example/fake" }, {
    frameId: 0,
    url: "https://video.example/page?private=1",
    tab: { id: 12, url: "https://www.netflix.com/watch/private" }
  });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "http://127.0.0.1:47653/media");
  assert.equal(requests[0].options.headers["Content-Type"], "application/json");
  const body = JSON.parse(requests[0].options.body);
  assert.deepEqual(Object.keys(body).sort(), ["kind", "playing", "sessionId", "site", "source", "spatial", "token"].sort());
  assert.equal(body.site, "www.netflix.com");
  assert.equal(body.kind, "movie");
  assert.equal(body.playing, true);
  assert.equal(body.spatial, false);
  assert.equal(body.sessionId, "chromium-12-stable-nonce");
  assert.equal(body.token, "a".repeat(64));
  assert.equal(JSON.stringify(body).includes("private"), false);
});

test("inactive browsing is not reported and play/pause reports stay ordered with one tab ID", async () => {
  const requests = [];
  let release;
  let nonce = 0;
  const context = {
    chrome: stubChrome(), URL, AbortSignal,
    crypto: { randomUUID: () => `nonce-${++nonce}` },
    fetch: async (url, options) => {
      requests.push(JSON.parse(options.body));
      if (requests.length === 1) await new Promise(resolve => { release = resolve; });
      return { ok: true };
    }
  };
  vm.runInNewContext(fs.readFileSync(path.join(root, "background.js"), "utf8"), context);
  await vm.runInNewContext('reportTab(99, "https://private.example/page", false)', context);
  assert.equal(requests.length, 0);
  const playing = vm.runInNewContext('reportTab(99, "https://netflix.com/watch/1", true)', context);
  await new Promise(resolve => setImmediate(resolve));
  const paused = vm.runInNewContext('reportTab(99, "https://netflix.com/watch/1", false)', context);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(requests.length, 1);
  release();
  await Promise.all([playing, paused]);
  assert.deepEqual(requests.map(report => report.playing), [true, false]);
  assert.equal(requests[0].sessionId, requests[1].sessionId);
  assert.equal(nonce, 1);
});

test("content script emits play, five-second heartbeat, and stop states", () => {
  const media = new FakeMedia();
  const events = {};
  const sent = [];
  let interval;
  const document = {
    visibilityState: "visible",
    addEventListener: (name, callback) => { events[`document:${name}`] = callback; },
    querySelectorAll: () => [media]
  };
  const window = {
    top: null,
    setInterval: (callback, ms) => { interval = callback; assert.equal(ms, 5000); return 1; },
    clearInterval: () => {},
    addEventListener: () => {}
  };
  window.top = window;
  const context = {
    window,
    document,
    location: { href: "https://www.netflix.com/watch/1" },
    HTMLMediaElement: FakeMedia,
    Element: class Element {},
    MutationObserver: class { observe() {} },
    chrome: { runtime: { sendMessage: (message) => sent.push(message) } },
    setTimeout,
    console
  };
  vm.runInNewContext(fs.readFileSync(path.join(root, "content.js"), "utf8"), context);
  media.paused = false;
  media.dispatch("play");
  assert.equal(sent.at(-1).playing, true);
  interval();
  assert.equal(sent.at(-1).playing, true);
  media.paused = true;
  interval();
  assert.equal(sent.at(-1).playing, false);
});

test("content script injection into an existing tab is guarded against duplicate listeners", () => {
  let observers = 0;
  const documentEvents = [];
  const window = { top: null, setInterval, clearInterval, addEventListener() {} };
  window.top = window;
  const context = {
    window,
    document: {
      visibilityState: "visible",
      addEventListener: (name) => documentEvents.push(name),
      querySelectorAll: () => []
    },
    location: { href: "https://www.netflix.com/watch/1" },
    HTMLMediaElement: FakeMedia,
    Element: class Element {},
    MutationObserver: class { constructor() { observers += 1; } observe() {} },
    chrome: { runtime: { sendMessage() {} } },
    console
  };
  const source = fs.readFileSync(path.join(root, "content.js"), "utf8");
  vm.runInNewContext(source, context);
  vm.runInNewContext(source, context);
  assert.equal(observers, 1);
  assert.equal(documentEvents.filter((name) => name === "DOMContentLoaded").length, 1);
});

test("pairing confirmation injects content.js into existing HTTP tabs", async () => {
  const existingTabs = [{ id: 4, url: "https://www.netflix.com/watch/1" }, { id: 5, url: "chrome://settings" }];
  const chrome = stubChrome(existingTabs);
  const context = { chrome, fetch: async () => ({ ok: true }), URL, crypto: { randomUUID: () => "nonce" }, AbortSignal };
  vm.runInNewContext(fs.readFileSync(path.join(root, "background.js"), "utf8"), context);
  const listener = chrome.runtime.messageListeners[0];
  let response;
  const keepChannelOpen = listener(
    { type: "pairing-saved" },
    { id: chrome.runtime.id, url: chrome.runtime.getURL("popup.html"), tab: { id: 20 } },
    (value) => { response = value; }
  );
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(keepChannelOpen, true);
  assert.equal(response.injected, 1);
  assert.equal(response.skipped, 0);
  assert.equal(chrome.injectedScripts.length, 1);
  assert.equal(chrome.injectedScripts[0].target.tabId, 4);
  assert.deepEqual(Array.from(chrome.injectedScripts[0].files), ["content.js"]);
});

test("popup confirms the pairing token before storing it and requests existing-tab injection", async () => {
  const stored = {};
  const formListeners = {};
  const status = { textContent: "" };
  const tokenInput = { value: "valid-token" };
  let request;
  let injectionRequest;
  const elements = {
    "pair-form": { addEventListener: (name, callback) => { formListeners[name] = callback; }, requestSubmit() { void formListeners.submit({ preventDefault() {} }); } },
    token: tokenInput,
    status
  };
  const context = {
    document: { getElementById: (id) => elements[id] },
    chrome: {
      storage: { local: { get: async () => ({}), set: async (values) => Object.assign(stored, values) } },
      runtime: { onMessage: { addListener() {} }, sendMessage: async (message) => { injectionRequest = message; return { injected: 2, skipped: 0 }; } }
    },
    fetch: async (url, options) => {
      request = { url, options };
      return { ok: true, json: async () => ({ ok: true }) };
    },
    AbortSignal,
    JSON,
    console
  };
  vm.runInNewContext(fs.readFileSync(path.join(root, "popup.js"), "utf8"), context);
  await formListeners.submit({ preventDefault() {} });
  assert.equal(request.url, "http://127.0.0.1:47653/pair");
  assert.equal(request.options.headers["Content-Type"], "application/json");
  assert.deepEqual(JSON.parse(request.options.body), { token: "valid-token" });
  assert.deepEqual(stored, { pairingToken: "valid-token" });
  assert.equal(injectionRequest.type, "pairing-saved");
  assert.match(status.textContent, /Connected/);
});

test("popup does not save an unconfirmed pairing key", async () => {
  const stored = {};
  const formListeners = {};
  const elements = {
    "pair-form": { addEventListener: (name, callback) => { formListeners[name] = callback; }, requestSubmit() {} },
    token: { value: "wrong-token" },
    status: { textContent: "" }
  };
  const context = {
    document: { getElementById: (id) => elements[id] },
    chrome: {
      storage: { local: { get: async () => ({}), set: async (values) => Object.assign(stored, values) } },
      runtime: { onMessage: { addListener() {} }, sendMessage: async () => ({ injected: 0, skipped: 0 }) }
    },
    fetch: async () => ({ ok: false, status: 403 }),
    AbortSignal,
    JSON,
    console
  };
  vm.runInNewContext(fs.readFileSync(path.join(root, "popup.js"), "utf8"), context);
  await formListeners.submit({ preventDefault() {} });
  assert.deepEqual(stored, {});
  assert.match(elements.status.textContent, /HTTP 403/);
});

class FakeMedia {
  constructor() {
    this.paused = true;
    this.ended = false;
    this.readyState = 4;
    this.isConnected = true;
    this.listeners = {};
  }
  addEventListener(name, callback) { this.listeners[name] = callback; }
  dispatch(name) { this.listeners[name]?.({ target: this }); }
}

function stubChrome(existingTabs = []) {
  const store = { "session:12": "chromium-12-stable-nonce" };
  const injectedScripts = [];
  const runtime = {
    id: "test-extension",
    getURL: (file) => `chrome-extension://test-extension/${file}`,
    messageListeners: [],
    onMessage: { addListener(listener) { runtime.messageListeners.push(listener); } },
    onInstalled: { addListener() {} },
    sendMessage() {}
  };
  return {
    runtime,
    tabs: {
      onUpdated: { addListener() {} },
      onRemoved: { addListener() {} },
      onActivated: { addListener() {} },
      query: async () => existingTabs,
      get: async () => null
    },
    storage: {
      local: { get: async () => ({ pairingToken: "a".repeat(64) }) },
      session: {
        get: async (key) => ({ [key]: store[key] }),
        set: async (values) => Object.assign(store, values),
        remove: async (key) => { delete store[key]; }
      }
    },
    scripting: { executeScript: async (options) => { injectedScripts.push(options); } },
    injectedScripts
  };
}
