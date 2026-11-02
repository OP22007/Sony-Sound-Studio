(() => {
  if (globalThis.__soundConnectMediaModeInstalled) return;
  globalThis.__soundConnectMediaModeInstalled = true;
  if (window.top !== window) return;

  const media = new Set();
  let heartbeatId = 0;
  let lastPlaying = false;
  let lastUrl = location.href;

  function playingNow() {
    for (const element of media) {
      if (!element.isConnected) {
        media.delete(element);
        continue;
      }
      if (!element.paused && !element.ended && element.readyState >= 2) return true;
    }
    return false;
  }

  function report(force = false) {
    const playing = playingNow();
    if (force || playing !== lastPlaying || location.href !== lastUrl) {
      lastPlaying = playing;
      lastUrl = location.href;
      chrome.runtime.sendMessage({ type: "media-state", playing, url: location.href });
    }
    if (playing && !heartbeatId) {
      heartbeatId = window.setInterval(() => {
        if (!playingNow()) {
          window.clearInterval(heartbeatId);
          heartbeatId = 0;
          report(true);
        } else {
          chrome.runtime.sendMessage({ type: "media-state", playing: true, url: location.href });
        }
      }, 5000);
    } else if (!playing && heartbeatId) {
      window.clearInterval(heartbeatId);
      heartbeatId = 0;
    }
  }

  function addMedia(element) {
    if (!(element instanceof HTMLMediaElement) || media.has(element)) return;
    media.add(element);
    for (const eventName of ["play", "playing", "pause", "ended", "emptied", "loadeddata"]) {
      element.addEventListener(eventName, () => report(), { passive: true });
    }
  }

  function scan(root = document) {
    if (root instanceof HTMLMediaElement) addMedia(root);
    if (root.querySelectorAll) root.querySelectorAll("video,audio").forEach(addMedia);
    report();
  }

  document.addEventListener("play", (event) => {
    addMedia(event.target);
    report();
  }, true);
  document.addEventListener("pause", (event) => {
    addMedia(event.target);
    report();
  }, true);
  document.addEventListener("ended", (event) => {
    addMedia(event.target);
    report();
  }, true);

  new MutationObserver((records) => {
    for (const record of records) {
      for (const node of record.addedNodes) {
        if (node instanceof Element) scan(node);
      }
    }
  }).observe(document, { childList: true, subtree: true });

  window.addEventListener("popstate", () => report(true));
  window.addEventListener("hashchange", () => report(true));
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") scan();
  });
  document.addEventListener("DOMContentLoaded", () => scan(), { once: true });
  window.addEventListener("pageshow", () => scan());
  scan();
})();
