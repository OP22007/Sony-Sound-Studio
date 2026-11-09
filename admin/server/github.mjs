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
