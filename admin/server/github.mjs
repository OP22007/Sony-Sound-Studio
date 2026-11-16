import { HttpError } from './security.mjs';

export function githubClient(token, cfg, fetcher = fetch) {
  return async function github(path, body, method = body ? 'POST' : 'GET') {
    const response = await fetcher(`https://api.github.com${path}`, {
      method, headers: { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'Sound-Studio-Owner-Admin', ...(body ? {'Content-Type':'application/json'} : {}) },
      ...(body ? {body: JSON.stringify(body)} : {}), signal: AbortSignal.timeout(20000),
    });
    if (!response.ok) {
      if ([401,403].includes(response.status)) throw new HttpError(response.status, 'GitHub denied access. Sign in again and check repository permissions.');
      if ([409,422].includes(response.status)) throw new HttpError(409, 'The branch changed or GitHub refused the update. Reload before saving again.');
      throw new HttpError(502, `GitHub request failed (${response.status}). No successful publication is assumed.`);
    }
    return response.status === 204 ? null : response.json();
  };
}
export async function requireOwner(github, cfg) {
  const user = await github('/user');
  if (user.id !== cfg.ownerId) throw new HttpError(403, 'Only the OP22007 owner account can access this admin.');
  return user;
}
export async function readSnapshot(github, cfg, branch = cfg.branch) {
  const ref = await github(`/repos/${cfg.repo}/git/ref/heads/${branch}`);
  const commit = await github(`/repos/${cfg.repo}/git/commits/${ref.object.sha}`);
  return { sha: ref.object.sha, tree: commit.tree.sha };
}
export async function readJSON(github, cfg, path, sha) {
  const file = await github(`/repos/${cfg.repo}/contents/${path}?ref=${sha}`);
  if (file.encoding !== 'base64' || !file.content) throw new HttpError(502, `Cannot read repository file ${path}.`);
  return JSON.parse(Buffer.from(file.content, 'base64').toString('utf8'));
}
export async function commitFiles(github, cfg, branch, snapshot, files, message) {
  const latest = await readSnapshot(github, cfg, branch);
  if (latest.sha !== snapshot.sha) throw new HttpError(409, 'The branch changed. Reload the latest data before saving.');
  const tree = await github(`/repos/${cfg.repo}/git/trees`, { base_tree: snapshot.tree, tree: [...files].map(([path, content]) => ({path, content, mode:'100644', type:'blob'})) });
  const commit = await github(`/repos/${cfg.repo}/git/commits`, { message, tree: tree.sha, parents: [snapshot.sha] });
  await github(`/repos/${cfg.repo}/git/refs/heads/${branch}`, {sha: commit.sha, force:false}, 'PATCH');
  return commit.sha;
}

// The admin never writes to the protected base branch. Edits live on one working branch that is
// proposed to the base branch through a pull request, so Vercel (built from the base) picks them up on merge.
export async function openPull(github, cfg) {
  const pulls = await github(`/repos/${cfg.repo}/pulls?state=open&head=${cfg.repo.split('/')[0]}:${cfg.branch}&base=${cfg.baseBranch}`);
  return Array.isArray(pulls) && pulls.length ? pulls[0] : null;
}
// Source of truth for editing: the working branch while its pull request is open, otherwise the base branch.
export async function workingSnapshot(github, cfg) {
  const pull = await openPull(github, cfg);
  return { ...(await readSnapshot(github, cfg, pull ? cfg.branch : cfg.baseBranch)), pull };
}
// Point the working branch at the snapshot before committing. With no open pull request the branch holds
// nothing unreviewed, so it may be created or moved to the base head.
export async function prepareBranch(github, cfg, snapshot) {
  if (snapshot.pull) return;
  const url = `/repos/${cfg.repo}/git/refs/heads/${cfg.branch}`;
  let exists = true;
  try { await github(`/repos/${cfg.repo}/git/ref/heads/${cfg.branch}`); } catch (error) { if (error.status !== 502) throw error; exists = false; }
  if (exists) await github(url, { sha: snapshot.sha, force: true }, 'PATCH');
  else await github(`/repos/${cfg.repo}/git/refs`, { ref: `refs/heads/${cfg.branch}`, sha: snapshot.sha });
}
export async function ensurePull(github, cfg, snapshot, modelId) {
  if (snapshot.pull) return snapshot.pull.html_url;
  try {
    const pull = await github(`/repos/${cfg.repo}/pulls`, { title: 'Update device compatibility from the owner admin', head: cfg.branch, base: cfg.baseBranch, body: `Owner compatibility review recorded in the admin app (latest: ${modelId}). Merging this updates the website on Vercel.

Further saves in the admin add commits to this pull request.` });
    return pull.html_url;
  } catch { return null; }
}
