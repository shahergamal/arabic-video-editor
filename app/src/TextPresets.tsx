import { useT, type TKey } from "./i18n";
import type { TextLayer } from "./types";

type Preset = Omit<TextLayer, "id" | "start" | "duration" | "font_family"> & { key: TKey; fontPref: string[] };

/** Starting points for common text. All are our own designs. */
export const PRESETS: Preset[] = [
  { key: "presetTitle", text: "عنوان رئيسي", fontPref: ["Lalezar", "Cairo"], size: 0.075, weight: 700, color: [255, 255, 255, 255], x: 0.5, y: 0.45, box_width: 0.9, shadow: true, outline: 0, outline_color: [0, 0, 0, 255], background: [0, 0, 0, 0], align: "center" },
  { key: "presetSubtitle", text: "عنوان فرعي", fontPref: ["Tajawal", "Cairo"], size: 0.042, weight: 400, color: [232, 163, 61, 255], x: 0.5, y: 0.55, box_width: 0.85, shadow: true, outline: 0, outline_color: [0, 0, 0, 255], background: [0, 0, 0, 0], align: "center" },
  { key: "presetLower", text: "الاسم · الوصف", fontPref: ["IBM Plex Sans Arabic", "Cairo"], size: 0.032, weight: 700, color: [255, 255, 255, 255], x: 0.5, y: 0.82, box_width: 0.8, shadow: false, outline: 0, outline_color: [0, 0, 0, 255], background: [28, 29, 32, 200], align: "center" },
  { key: "presetCaption", text: "سطر الترجمة هنا", fontPref: ["Almarai", "Cairo"], size: 0.038, weight: 700, color: [255, 255, 255, 255], x: 0.5, y: 0.72, box_width: 0.86, shadow: false, outline: 0.06, outline_color: [0, 0, 0, 255], background: [0, 0, 0, 0], align: "center" },
];

export function TextPresets({ families, brandFont, onAdd }: { families: string[]; brandFont?: string | null; onAdd: (t: Omit<TextLayer, "id" | "start" | "duration">) => void }) {
  const t = useT();
  return (
    <div className="panel-body presets">
      {PRESETS.map(({ key, fontPref, ...p }) => {
        const font_family = [brandFont ?? "", ...fontPref].find((f) => families.includes(f)) ?? families[0] ?? "Cairo";
        const css = `rgb(${p.color.slice(0, 3).join(",")})`;
        return (
          <button key={key} className="preset" onClick={() => onAdd({ ...p, font_family })}>
            <span
              className="preset-sample"
              style={{
                fontFamily: `"${font_family}"`,
                color: css,
                fontWeight: p.weight,
                background: p.background[3] ? `rgba(${p.background.slice(0, 3).join(",")},${p.background[3] / 255})` : undefined,
                WebkitTextStroke: p.outline ? "4px black" : undefined,
                paintOrder: "stroke fill",
              }}
            >
              {p.text}
            </span>
            <span className="muted">{t(key)}</span>
          </button>
        );
      })}
    </div>
  );
}
