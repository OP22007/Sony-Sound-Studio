import { generateCompatibility } from '../../admin/lib/compatibility.mjs';
import { readFile, writeFile, mkdir, copyFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const root = new URL('../../', import.meta.url);
const registry = JSON.parse(await readFile(new URL('docs/compatibility/devices.json', root), 'utf8'));
const artwork = JSON.parse(await readFile(new URL('docs/compatibility/artwork.json', root), 'utf8'));

function assert(condition, message) { if (!condition) throw new Error(message); }
assert(new Set(artwork.images.map(record => record.model)).size === artwork.images.length, 'duplicate artwork provenance');
for (const record of artwork.images) {
  assert(registry.devices.some(model => model.id === record.model && model.artwork === record.filename), `artwork/model mismatch ${record.model}`);
  assert(record.width >= 100 && record.height >= 100 && record.reviewedOn, `unreviewed or undersized artwork ${record.model}`);
  assert(new URL(record.sourcePage || record.imageUrl).protocol === 'https:', `invalid artwork source ${record.model}`);
  const bytes = await readFile(new URL(`website/public/images/${record.filename}`, root));
  assert(createHash('sha256').update(bytes).digest('hex') === record.sha256, `artwork digest mismatch ${record.model}`);
}

for (const model of registry.devices) if (model.artwork) await readFile(new URL(`website/public/images/${model.artwork}`, root));
for (const record of Object.values(registry.evidence)) await readFile(new URL(record.url.split('#')[0], new URL('docs/compatibility/', root)));
const files = generateCompatibility(registry, artwork, process.env.SITE_URL);
for (const [path, content] of files) {
  const target = new URL(path.startsWith('docs/') ? path : `website/public/${path}`, root);
  if (path.startsWith('docs/') && process.argv.includes('--check')) {
    assert((await readFile(target, 'utf8')).replace(/\r\n/g, '\n') === content, 'Generated GitHub table is stale; run npm run devices');
  } else {
    await mkdir(new URL('./', target), {recursive:true});
    await writeFile(target, content);
  }
}
await copyFile(new URL('website/node_modules/@fontsource-variable/manrope/files/manrope-latin-wght-normal.woff2', root), new URL('website/public/devices/manrope.woff2', root));
console.log(`Validated and generated ${registry.devices.length} static model pages, directory and GitHub feature matrix.`);
