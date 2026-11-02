import { readdir, mkdir, copyFile, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';

const target = process.argv[2];
const expected = {
  'aarch64-apple-darwin': ['.dmg'],
  'x86_64-apple-darwin': ['.dmg'],
  'x86_64-unknown-linux-gnu': ['.deb', '.rpm', '.AppImage'],
  'x86_64-pc-windows-msvc': ['.exe'],
}[target];
if (!expected) throw new Error('Unknown release target');
const root = path.resolve('src-tauri/target', target, 'release/bundle');
const destination = path.resolve('src-tauri/target/release-assets', target);
await mkdir(destination, { recursive: true });
const collected = [];
async function visit(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const source = path.join(directory, entry.name);
    if (entry.isDirectory() && !entry.name.endsWith('.app')) await visit(source);
    else if (entry.isFile() && expected.some(extension => entry.name.endsWith(extension))) {
      // Include architecture explicitly and use URL-safe names across all installers.
      const name = `${target}-${entry.name.replaceAll(' ', '.')}`;
      await copyFile(source, path.join(destination, name));
      const hash = createHash('sha256').update(await readFile(source)).digest('hex');
      collected.push({ name, hash });
    }
  }
}
await visit(root);
for (const extension of expected) {
  if (!collected.some(file => file.name.endsWith(extension))) throw new Error(`Missing ${extension} package for ${target}`);
}
collected.sort((a, b) => a.name.localeCompare(b.name));
await writeFile(path.join(destination, `SHA256SUMS-${target}.txt`), collected.map(file => `${file.hash}  ${file.name}\n`).join(''));
console.log(`Collected ${collected.length} installers for ${target}`);
