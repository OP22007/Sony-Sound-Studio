import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const execFileAsync = promisify(execFile);
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const collector = path.join(repoRoot, 'scripts', 'collect-release-assets.mjs');
const bundles = [
  {
    target: 'aarch64-apple-darwin',
    files: ['dmg/demo.dmg', 'macos/demo.app.tar.gz', 'macos/demo.app.tar.gz.sig'],
    updater: 'demo.app.tar.gz',
  },
  {
    target: 'x86_64-apple-darwin',
    files: ['dmg/demo.dmg', 'macos/demo.app.tar.gz', 'macos/demo.app.tar.gz.sig'],
    updater: 'demo.app.tar.gz',
  },
  {
    target: 'x86_64-unknown-linux-gnu',
    files: ['deb/demo.deb', 'rpm/demo.rpm', 'appimage/demo.AppImage', 'appimage/demo.AppImage.sig'],
    updater: 'demo.AppImage',
  },
  {
    target: 'x86_64-pc-windows-msvc',
    files: ['nsis/demo-setup.exe', 'nsis/demo-setup.exe.sig'],
    updater: 'demo-setup.exe',
  },
];

test('collects installer bundles and signed updater artifacts for all release targets', async t => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'sound-studio-release-assets-'));
  t.after(() => rm(workspace, { recursive: true, force: true }));
  for (const bundle of bundles) {
    const root = path.join(workspace, 'src-tauri', 'target', bundle.target, 'release', 'bundle');
    for (const [index, relative] of bundle.files.entries()) {
      const source = path.join(root, relative);
      await mkdir(path.dirname(source), { recursive: true });
      await writeFile(source, `fixture-${bundle.target}-${index}`);
    }
  }

  for (const bundle of bundles) {
    await execFileAsync(process.execPath, [collector, bundle.target], { cwd: workspace });
    const outputDir = path.join(workspace, 'src-tauri', 'target', 'release-assets', bundle.target);
    const updaterName = `${bundle.target}-${bundle.updater}`;
    const signatureName = `${updaterName}.sig`;
    await readFile(path.join(outputDir, updaterName));
    await readFile(path.join(outputDir, signatureName));
    const checksum = await readFile(path.join(outputDir, `SHA256SUMS-${bundle.target}.txt`), 'utf8');
    assert.match(checksum, new RegExp(`  ${updaterName.replaceAll('.', '\\.')}\\n`));
    assert.match(checksum, new RegExp(`  ${signatureName.replaceAll('.', '\\.')}\\n`));
  }
});
