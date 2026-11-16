// Downloads every font listed in assets/fonts/manifest.json, with its license file.
// Usage: node scripts/fetch-fonts.mjs
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "assets", "fonts");
const manifest = JSON.parse(await readFile(join(root, "manifest.json"), "utf8"));

async function download(url, dest) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${url}`);
  await writeFile(dest, Buffer.from(await res.arrayBuffer()));
}

for (const font of manifest.fonts) {
  const dir = join(root, font.id);
  await mkdir(dir, { recursive: true });
  for (const url of font.files) {
    const name = decodeURIComponent(url.split("/").pop());
    await download(url, join(dir, name));
  }
  await download(font.license_file, join(dir, "LICENSE.txt"));
  console.log(`ok  ${font.family}`);
}
