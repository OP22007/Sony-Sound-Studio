import { randomBytes, createHash, createCipheriv, createDecipheriv, timingSafeEqual } from 'node:crypto';

export class HttpError extends Error {
  constructor(status, message) { super(message); this.status = status; }
}
export function config(env = process.env) {
  for (const name of ['ADMIN_ORIGIN', 'GITHUB_CLIENT_ID', 'GITHUB_CLIENT_SECRET', 'SESSION_SECRET']) {
    if (!env[name]) throw new HttpError(503, `Admin setup incomplete: ${name} is required.`);
  }
  const origin = new URL(env.ADMIN_ORIGIN);
  const local = origin.protocol === 'http:' && ['127.0.0.1', 'localhost'].includes(origin.hostname) && env.NODE_ENV !== 'production';
  if ((!local && origin.protocol !== 'https:') || origin.pathname !== '/' || origin.search || origin.hash || origin.username || origin.password) throw new HttpError(503, 'ADMIN_ORIGIN must be an HTTPS origin without a path.');
  if (!/^[a-fA-F0-9]{64}$/.test(env.SESSION_SECRET)) throw new HttpError(503, 'SESSION_SECRET must contain 32 random bytes encoded as 64 hex characters.');
  const branch = env.SOURCE_BRANCH || 'codex/website-scroll-launch';
  const authMode = env.GITHUB_AUTH_MODE || 'oauth-app';
  if (!['oauth-app', 'github-app'].includes(authMode)) throw new HttpError(503, 'GITHUB_AUTH_MODE must be oauth-app or github-app.');
  if (!/^codex\/[a-zA-Z0-9-]+$/.test(branch)) throw new HttpError(503, 'SOURCE_BRANCH must be a codex/ working branch.');
  return { origin: origin.origin, local, authMode, clientId: env.GITHUB_CLIENT_ID, clientSecret: env.GITHUB_CLIENT_SECRET, key: Buffer.from(env.SESSION_SECRET, 'hex'), ownerId: 140067318, branch, repo: 'OP22007/Sony-Sound-Studio', repoId: 1441020213 };
}
export const random = () => randomBytes(32).toString('base64url');
export const challenge = value => createHash('sha256').update(value).digest('base64url');
export function equal(a, b) {
  return typeof a === 'string' && typeof b === 'string' && a.length > 0 && Buffer.byteLength(a) === Buffer.byteLength(b) && timingSafeEqual(Buffer.from(a), Buffer.from(b));
}
export function seal(value, cfg, purpose, seconds = 14400) {
  const iv = randomBytes(12), cipher = createCipheriv('aes-256-gcm', cfg.key, iv);
  cipher.setAAD(Buffer.from(`${cfg.origin}:${purpose}`));
  const encrypted = Buffer.concat([cipher.update(JSON.stringify({ ...value, exp: Date.now() + seconds * 1000 })), cipher.final()]);
  return Buffer.concat([iv, cipher.getAuthTag(), encrypted]).toString('base64url');
}
export function unseal(value, cfg, purpose) {
  try {
    if (!value || value.length > 7000) return null;
    const data = Buffer.from(value, 'base64url');
    const cipher = createDecipheriv('aes-256-gcm', cfg.key, data.subarray(0, 12));
    cipher.setAAD(Buffer.from(`${cfg.origin}:${purpose}`)); cipher.setAuthTag(data.subarray(12, 28));
    const payload = JSON.parse(Buffer.concat([cipher.update(data.subarray(28)), cipher.final()]));
    return payload.exp > Date.now() ? payload : null;
  } catch { return null; }
}
export function cookieName(cfg, purpose) { return `${cfg.local ? '' : '__Host-'}studio_${purpose}`; }
export function cookie(cfg, purpose, value, age = 14400) {
  return `${cookieName(cfg, purpose)}=${value}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${age}${cfg.local ? '' : '; Secure'}`;
}
export function readCookie(req, cfg, purpose) {
  const name = cookieName(cfg, purpose);
  return (req.headers.cookie || '').split(';').map(v => v.trim()).find(v => v.startsWith(name + '='))?.slice(name.length + 1);
}
export function checkMutation(req, session, cfg) {
  if (req.headers.origin !== cfg.origin || !equal(req.headers['x-csrf-token'], session.csrf)) throw new HttpError(403, 'Invalid request origin or security token. Reload the admin page.');
  if (!(req.headers['content-type'] || '').startsWith('application/json')) throw new HttpError(415, 'JSON is required.');
}
export function securityHeaders(res) {
  for (const [key, value] of Object.entries({
    'Cache-Control': 'no-store', 'X-Robots-Tag': 'noindex, nofollow, noarchive',
    'X-Content-Type-Options': 'nosniff', 'X-Frame-Options': 'DENY', 'Referrer-Policy': 'no-referrer',
    'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' https://op22007.github.io; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
  })) res.setHeader(key, value);
}
