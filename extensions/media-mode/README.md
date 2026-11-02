# Sound Connect Media Mode (Chromium)

This unpacked Chromium Manifest V3 extension reports media playback state to the Sound Connect desktop app at `http://127.0.0.1:47653/media`.

## Install

1. Open the **Automatic listening** tab, enable **Follow media**, expand **Browser companion setup**, and copy its pairing key. Choose **Prepare Chrome / Edge extension** to export the bundled extension and open its folder.
2. Open `chrome://extensions` (or the equivalent extensions page in a Chromium browser) and enable **Developer mode**.
3. Choose **Load unpacked** and select the prepared extension folder (or this `media-mode` source directory when developing).
4. Open the extension popup and enter the desktop app's pairing token, then choose **Connect**. The extension confirms the token with the desktop before saving it and then attaches media detection to existing web tabs where browser permissions allow it. Saved tokens are checked with the desktop each time the popup opens.

Enable **Follow media** in Automatic listening before pairing. The desktop receiver accepts authenticated JSON reports from extension origins and handles the browser's CORS preflight. It is closed while Follow media is off.

## Reports and privacy

Each POST contains only the pairing `token`, a fixed `source` value, the page hostname in `site`, a coarse `kind`, `playing`, `spatial: false`, and a per-tab `sessionId`. It never sends page titles, media titles, or full URLs. The content script observes HTML `<video>` and `<audio>` playback in the top-level document. While playback continues it reports every five seconds; it also reports on play/pause/end and tab navigation. The background worker associates messages with the sender tab and clears that tab's session ID when the tab closes.

The extension classifies exact, listed streaming hosts as `movie` (including Netflix) and listed music services as `music`. Other hosts are always `other`, even if they play video. This allowlist is deliberately modest and may need updates as services change domains. Classification is based on the hostname, not the media's content. `spatial` is always false; the extension does not detect Atmos or other spatial formats.

## Firefox package

Choose **Prepare Firefox extension** in the app. Alternatively, run `node extensions/media-mode/package-firefox.mjs` from the repository root to prepare `extensions/media-mode-firefox`. In Firefox, open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on**, and select the prepared folder's `manifest.json`. Temporary add-ons must be loaded again after Firefox restarts. Firefox packaging is provided, but it has not been runtime-tested here. Browser-managed pages, some protected media surfaces, and media rendered only in cross-origin frames may not be observable.
