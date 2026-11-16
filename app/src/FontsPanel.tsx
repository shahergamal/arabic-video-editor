import { useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useT, type TKey } from "./i18n";
import type { FontInfo } from "./types";

const CATEGORIES = ["all", "modern", "kufi", "naskh", "ruqaa", "nastaliq", "display", "handwritten"] as const;

interface Props {
  fonts: FontInfo[];
  current?: string;
  onUse?: (family: string) => void;
  onAddFont: (path: string) => void;
}

/** Every font with a live preview of your own sentence. */
export function FontsPanel({ fonts, current, onUse, onAddFont }: Props) {
  const t = useT();
  const [q, setQ] = useState("");
  const [cat, setCat] = useState<(typeof CATEGORIES)[number]>("all");
  const [sample, setSample] = useState("شقة للبيع بإطلالة مميزة ١٢٣");
  const [showSystem, setShowSystem] = useState(false);
  const [favs, setFavs] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem("favFonts") ?? "[]");
    } catch {
      return [];
    }
  });
  const toggleFav = (f: string) => {
    const next = favs.includes(f) ? favs.filter((x) => x !== f) : [...favs, f];
    setFavs(next);
    try {
      localStorage.setItem("favFonts", JSON.stringify(next));
    } catch {
      /* not remembered in private mode */
    }
  };

  const list = useMemo(() => {
    const ql = q.trim().toLowerCase();
    return fonts
      .filter((f) => (showSystem ? true : f.origin !== "system"))
      .filter((f) => cat === "all" || f.category === cat)
      .filter((f) => !ql || f.family.toLowerCase().includes(ql) || t(f.category as TKey)?.includes(q.trim()))
      .sort((a, b) => Number(favs.includes(b.family)) - Number(favs.includes(a.family)));
  }, [fonts, q, cat, favs, showSystem, t]);

  const pick = async () => {
    const p = await openDialog({ filters: [{ name: "Font", extensions: ["ttf", "otf", "ttc", "woff2", "woff"] }] });
    if (typeof p === "string") onAddFont(p);
  };

  return (
    <div className="panel-body fonts">
      <input placeholder={t("searchFonts")} value={q} onChange={(e) => setQ(e.target.value)} dir="auto" />
      <label>
        {t("previewText")}
        <input value={sample} onChange={(e) => setSample(e.target.value)} dir="auto" />
      </label>
      <div className="chips">
        {CATEGORIES.map((c) => (
          <button key={c} className={cat === c ? "on" : ""} onClick={() => setCat(c)}>
            {t(c)}
          </button>
        ))}
      </div>
      <label className="check">
        <input type="checkbox" checked={showSystem} onChange={(e) => setShowSystem(e.target.checked)} />
        {t("system")}
      </label>
      <button onClick={pick}>{t("addFont")}</button>
      <ul className="font-list">
        {list.map((f) => (
          <li key={f.family} className={current === f.family ? "current" : ""}>
            <div className="font-head">
              <button className={`star ${favs.includes(f.family) ? "on" : ""}`} onClick={() => toggleFav(f.family)}>
                ★
              </button>
              <span className="family">{f.family}</span>
              <span className="origin">{t(f.origin)}</span>
            </div>
            <div className="sample" style={{ fontFamily: `"${f.family}"` }} dir="auto">
              {sample}
            </div>
            <div className="font-foot">
              {f.license && (
                <span className="license" title={f.commercial_use ? t("commercialOk") : ""}>
                  {f.license}
                  {f.commercial_use ? " ✓" : ""}
                </span>
              )}
              {onUse && (
                <button className="use" onClick={() => onUse(f.family)}>
                  {t("useFont")}
                </button>
              )}
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}
