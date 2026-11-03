import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { assertNotOlder, compareVersions, generateManifest } from './generate-updater-manifest.mjs';

const updaterFiles = [
  ['aarch64-apple-darwin', 'Sound Studio.app.tar.gz'],
  ['x86_64-apple-darwin', 'Sound Studio.app.tar.gz'],
  ['x86_64-unknown-linux-gnu', 'Sound Studio.AppImage'],
  ['x86_64-pc-windows-msvc', 'Sound Studio-setup.exe'],
];

async function makeAssets() {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'sound-studio-updater-'));
  for (const [target, filename] of updaterFiles) {
    const artifact = `${target}-${filename}`;
    await writeFile(path.join(dir, artifact), 'package');
    await writeFile(path.join(dir, `${artifact}.sig`), `signed:${target}\n`);
  }
  return dir;
}

test('creates a complete static updater feed for all four targets', async t => {
  const assetsDir = await makeAssets();
  t.after(() => rm(assetsDir, { recursive: true, force: true }));
  const manifest = await generateManifest({
    assetsDir,
    version: 'v0.1.2',
    notes: 'Release notes',
    pubDate: '2026-11-02T12:30:00Z',
    baseUrl: 'https://github.com/example/repo/releases/download/updater-preview',
  });
  assert.equal(manifest.version, '0.1.2');
  assert.equal(manifest.notes, 'Release notes');
  assert.equal(manifest.pub_date, '2026-11-02T12:30:00.000Z');
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [
    'darwin-aarch64', 'darwin-x86_64', 'linux-x86_64', 'windows-x86_64',
  ]);
  assert.equal(manifest.platforms['darwin-aarch64'].signature, 'signed:aarch64-apple-darwin');
  assert.match(manifest.platforms['darwin-aarch64'].url, /aarch64-apple-darwin-Sound%20Studio\.app\.tar\.gz$/);
});

test('rejects an updater platform without a signature', async t => {
  const assetsDir = await makeAssets();
  t.after(() => rm(assetsDir, { recursive: true, force: true }));
  await rm(path.join(assetsDir, 'x86_64-pc-windows-msvc-Sound Studio-setup.exe.sig'));
  await assert.rejects(generateManifest({
    assetsDir, version: '0.1.2', notes: '', pubDate: '2026-11-02T00:00:00Z', baseUrl: 'https://example.test',
  }), /ENOENT/);
});

test('compares stable semantic versions and rejects prerelease tags', () => {
  assert.equal(compareVersions('0.1.9', '0.1.10'), -1);
  assert.equal(compareVersions('v0.1.2', '0.1.2'), 0);
  assert.throws(() => compareVersions('0.1.2', '0.1.2-rc.1'), /Invalid stable semantic version/);
  assert.throws(() => compareVersions('0.1.2-rc.2', '0.1.2'), /Invalid stable semantic version/);
  assert.throws(() => compareVersions('latest', '0.1.2'), /Invalid stable semantic version/);
  assert.doesNotThrow(() => assertNotOlder('0.1.10', '0.1.9'));
  assert.throws(() => assertNotOlder('0.1.1', '0.1.2'), /Refusing to downgrade/);
});
