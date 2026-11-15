// Downloads ready-made ffmpeg + ffprobe for this computer into
// app/src-tauri/binaries, named the way the app bundler expects.
// Usage: node scripts/fetch-ffmpeg.mjs [target-triple]
//
// These are GPL builds (they include the x264 encoder). That is fine for
// personal use. Before selling the app, switch to LGPL builds and the
// system's own encoders (see docs/licenses.md).
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync, copyFileSync, chmodSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "app", "src-tauri", "binaries");
const triple =
  process.argv[2] ?? execFileSync("rustc", ["-vV"], { encoding: "utf8" }).match(/host: (\S+)/)[1];

const btbn = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest";
const riedl = "https://ffmpeg.martin-riedl.de/redirect/latest/macos";
const sources = {
  "x86_64-pc-windows-msvc": [`${btbn}/ffmpeg-master-latest-win64-gpl.zip`],
  "x86_64-unknown-linux-gnu": [`${btbn}/ffmpeg-master-latest-linux64-gpl.tar.xz`],
  "aarch64-apple-darwin": [`${riedl}/arm64/release/ffmpeg.zip`, `${riedl}/arm64/release/ffprobe.zip`],
  "x86_64-apple-darwin": [`${riedl}/amd64/release/ffmpeg.zip`, `${riedl}/amd64/release/ffprobe.zip`],
}[triple];
if (!sources) throw new Error(`No ffmpeg download known for ${triple}`);

const work = mkdtempSync(join(tmpdir(), "ffmpeg-"));
for (const url of sources) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} ${url}`);
  const file = join(work, url.endsWith(".tar.xz") ? "pkg.tar.xz" : `pkg${sources.indexOf(url)}.zip`);
  writeFileSync(file, Buffer.from(await res.arrayBuffer()));
  // bsdtar (Windows 10+, macOS) and GNU tar both unpack these.
  if (file.endsWith(".zip") && process.platform !== "win32" && process.platform !== "darwin") {
    execFileSync("unzip", ["-o", "-q", file, "-d", work]);
  } else {
    execFileSync("tar", ["-xf", file, "-C", work]);
  }
}

function find(dir, name) {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) {
      const hit = find(p, name);
      if (hit) return hit;
    } else if (entry === name) return p;
  }
  return null;
}

mkdirSync(out, { recursive: true });
const ext = triple.includes("windows") ? ".exe" : "";
for (const tool of ["ffmpeg", "ffprobe"]) {
  const src = find(work, tool + ext);
  if (!src) throw new Error(`${tool} not found in download`);
  const dest = join(out, `${tool}-${triple}${ext}`);
  copyFileSync(src, dest);
  chmodSync(dest, 0o755);
  console.log(`ok  ${dest}`);
}
