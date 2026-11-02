import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const output = path.resolve(here, "..", "media-mode-firefox");
const files = ["background.js", "content.js", "popup.html", "popup.js", "README.md"];

await mkdir(output, { recursive: true });
for (const file of files) await cp(path.join(here, file), path.join(output, file));
const manifest = await readFile(path.join(here, "manifest.firefox.json"), "utf8");
await writeFile(path.join(output, "manifest.json"), manifest);
console.log(`Firefox extension package prepared at ${output}`);
