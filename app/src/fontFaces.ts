// Lets the window draw previews in the bundled and added fonts by pointing
// CSS @font-face rules at the font files on disk.
import { convertFileSrc } from "@tauri-apps/api/core";
import type { FontInfo } from "./types";

let style: HTMLStyleElement | null = null;

export function registerFontFaces(fonts: FontInfo[]) {
  if (!style) {
    style = document.createElement("style");
    document.head.appendChild(style);
  }
  const rules: string[] = [];
  for (const f of fonts) {
    for (const file of f.files) {
      const fmt = file.toLowerCase().endsWith(".otf") ? "opentype" : "truetype";
      // Variable fonts cover every weight; static files are labelled by name.
      const w = /Black/i.test(file) ? 900 : /ExtraBold/i.test(file) ? 800 : /SemiBold/i.test(file) ? 600 : /Bold/i.test(file) ? 700 : /Medium/i.test(file) ? 500 : /ExtraLight/i.test(file) ? 200 : /Light/i.test(file) ? 300 : 400;
      const weight = file.includes("[") ? "100 900" : String(w);
      rules.push(`@font-face{font-family:"${f.family}";src:url("${convertFileSrc(file)}") format("${fmt}");font-weight:${weight};font-display:swap}`);
    }
  }
  style.textContent = rules.join("\n");
}
