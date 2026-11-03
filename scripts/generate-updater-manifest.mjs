import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const platforms = {
  'aarch64-apple-darwin': 'darwin-aarch64',
  'x86_64-apple-darwin': 'darwin-x86_64',
  'x86_64-unknown-linux-gnu': 'linux-x86_64',
  'x86_64-pc-windows-msvc': 'windows-x86_64',
};

export function compareVersions(left, right) {
  const parse = value => {
    const match = /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:\+[0-9A-Za-z.-]+)?$/.exec(value);
    if (!match) throw new Error(`Invalid stable semantic version: ${value}`);
    return [Number(match[1]), Number(match[2]), Number(match[3])];
  };
  const a = parse(left), b = parse(right);
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return Math.sign(a[i] - b[i]);
  return 0;
}

export function assertNotOlder(candidate, current) {
  if (compareVersions(candidate, current) < 0) {
    throw new Error(`Refusing to downgrade updater feed from ${current} to ${candidate}`);
  }
}

export async function generateManifest({ assetsDir, version, notes, pubDate, baseUrl }) {
  if (compareVersions(version, version) !== 0) throw new Error('Invalid version');
  const assets = await readdir(assetsDir);
  const platformsManifest = {};
  for (const [target, key] of Object.entries(platforms)) {
    const artifactSuffix = target.includes('apple-darwin') ? '.app.tar.gz'
      : target.includes('linux') ? '.AppImage' : '.exe';
    const artifacts = assets.filter(name => name.startsWith(`${target}-`) && name.endsWith(artifactSuffix));
    if (artifacts.length !== 1) throw new Error(`Expected exactly one ${artifactSuffix} updater artifact for ${target}, found ${artifacts.length}`);
    const artifact = artifacts[0];
    const signaturePath = path.join(assetsDir, `${artifact}.sig`);
    const signature = (await readFile(signaturePath, 'utf8')).trim();
    if (!signature) throw new Error(`Empty updater signature for ${artifact}`);
    platformsManifest[key] = {
      signature,
      url: `${baseUrl.replace(/\/$/, '')}/${encodeURIComponent(artifact)}`,
    };
  }
  const date = new Date(pubDate);
  if (!Number.isFinite(date.valueOf())) throw new Error(`Invalid publication date: ${pubDate}`);
  return { version: version.replace(/^v/, ''), notes, pub_date: date.toISOString(), platforms: platformsManifest };
}

export async function main(args = process.argv.slice(2)) {
  const [assetsDir, version, notesFile, pubDate, baseUrl, outputPath, previousManifestPath] = args;
  if (!assetsDir || !version || !notesFile || !pubDate || !baseUrl || !outputPath) {
    throw new Error('Usage: node scripts/generate-updater-manifest.mjs <assets-dir> <version> <notes-file> <pub-date> <base-url> <output-file>');
  }
  const notes = await readFile(notesFile, 'utf8');
  const manifest = await generateManifest({ assetsDir, version, notes, pubDate, baseUrl });
  if (previousManifestPath) {
    const previous = JSON.parse(await readFile(previousManifestPath, 'utf8'));
    assertNotOlder(manifest.version, previous.version);
  }
  await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
