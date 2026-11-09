type Platform = 'windows' | 'macos' | 'linux' | 'other';

export function detectPlatform(nav: Navigator): Platform {
  const hints = (nav as Navigator & { userAgentData?: { mobile?: boolean; platform?: string } }).userAgentData;
  const ua = nav.userAgent;
  // iPadOS can identify as a Mac. Never recommend a desktop installer on tablets.
  if (hints?.mobile || /Android|iPhone|iPad|iPod|CrOS/i.test(ua) ||
      (/Mac/i.test(nav.platform) && nav.maxTouchPoints > 1)) return 'other';
  const platform = hints?.platform || nav.platform || ua;
  if (/Win/i.test(platform)) return 'windows';
  if (/Mac/i.test(platform)) return 'macos';
  if (/Linux|X11/i.test(platform)) return 'linux';
  return 'other';
}

export function configureDownloads() {
  const platform = detectPlatform(navigator);
  const link = document.querySelector<HTMLAnchorElement>('[data-os-download]');
  const label = document.querySelector('[data-os-download-label]');
  if (!link || !label || platform === 'other') return;
  const card = document.querySelector<HTMLAnchorElement>(`[data-platform="${platform}"]`);
  if (!card) return;
  // A Mac's browser does not reliably identify Intel versus Apple Silicon.
  link.href = platform === 'macos' ? '#download-macos' : card.href;
  label.textContent = platform === 'macos' ? 'Choose your Mac preview' :
    platform === 'windows' ? 'Get the Windows preview' : 'Get the Linux preview';
  card.classList.add('is-recommended');
  const badge = document.createElement('span');
  badge.className = 'platform-recommendation';
  badge.textContent = 'Suggested for this device';
  card.append(badge);
}
