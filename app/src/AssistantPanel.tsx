import { useState } from "react";
import { api } from "./api";
import { cutSilences, listingTexts, publishCheck, type Listing } from "./assistant";
import { useT, type TKey } from "./i18n";
import type { Brand, Clip, Project } from "./types";
import { allClips, fileName } from "./types";

interface Props {
  project: Project;
  selected: Clip | null;
  missing: string[];
  brand: Brand;
  families: string[];
  onCommit: (p: Project) => void;
  onJump: (frame: number) => void;
}

const LEVELS = [
  { key: "quietLight", db: -40, min: 0.8 },
  { key: "quietNormal", db: -32, min: 0.5 },
  { key: "quietStrong", db: -26, min: 0.35 },
] as const;

export function AssistantPanel({ project, selected, missing, brand, families, onCommit, onJump }: Props) {
  const t = useT();
  const [level, setLevel] = useState(1);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [checked, setChecked] = useState(false);
  const [listing, setListing] = useState<Listing>({ kind: "شقة للبيع", place: "", price: "", area: "", rooms: "", extra: "", phone: "" });

  // The clip to clean: the chosen one, else the first talking clip.
  const target = selected && selected.media !== "image" ? selected : allClips(project).find((c) => c.media === "video" && !c.speed_ramp?.length) ?? null;

  const cut = async () => {
    if (!target) return;
    setBusy(true);
    setNote(null);
    try {
      const l = LEVELS[level];
      const quiet = await api.silences(target.source, l.db, l.min);
      const { project: next, removed } = cutSilences(project, target.id, quiet);
      if (removed > 0) {
        onCommit(next);
        setNote(`${t("silencesCut")} ${(removed / (project.fps.num / project.fps.den)).toFixed(1)} ${t("seconds")}`);
      } else setNote(t("noSilences"));
    } catch (e) {
      setNote(String(e));
    } finally {
      setBusy(false);
    }
  };

  const issues = checked ? publishCheck(project, missing) : [];
  const font = [brand.font ?? "", "Lalezar", "Cairo"].find((f) => families.includes(f)) ?? families[0] ?? "Cairo";
  const accent = ((): [number, number, number, number] => {
    const hex = brand.colors[0];
    if (!hex) return [232, 163, 61, 255];
    const n = parseInt(hex.replace("#", ""), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 255];
  })();
  const field = (k: keyof Listing, label: TKey, opts: { ltr?: boolean; area?: boolean } = {}) => (
    <label key={k}>
      {t(label)}
      {opts.area ? (
        <textarea rows={2} dir="auto" value={listing[k]} onChange={(e) => setListing({ ...listing, [k]: e.target.value })} />
      ) : (
        <input dir={opts.ltr ? "ltr" : "auto"} value={listing[k]} onChange={(e) => setListing({ ...listing, [k]: e.target.value })} />
      )}
    </label>
  );

  return (
    <div className="panel-body assistant">
      <h4>{t("cutSilences")}</h4>
      <p className="hint small">{t("cutSilencesHint")}</p>
      {target ? (
        <p className="muted small" dir="auto">
          {fileName(target.source)}
        </p>
      ) : (
        <p className="hint small">{t("needVideo")}</p>
      )}
      <div className="segmented">
        {LEVELS.map((l, i) => (
          <button key={l.key} className={level === i ? "on" : ""} onClick={() => setLevel(i)}>
            {t(l.key)}
          </button>
        ))}
      </div>
      <button className="primary" disabled={!target || busy} onClick={cut}>
        {busy ? t("working") : t("cutNow")}
      </button>
      {note && (
        <p className="hint small" dir="auto">
          {note}
        </p>
      )}

      <h4>{t("publishCheck")}</h4>
      <button onClick={() => setChecked(true)}>{checked ? t("checkAgain") : t("checkNow")}</button>
      {checked &&
        (issues.length === 0 ? (
          <p className="ok small">✓ {t("allGood")}</p>
        ) : (
          <ul className="issues">
            {issues.map((i, n) => (
              <li key={n}>
                <span>
                  {t(i.key)}
                  {i.detail && (
                    <span className="muted" dir="auto">
                      {" "}
                      ({i.detail})
                    </span>
                  )}
                </span>
                {i.frame != null && <button onClick={() => onJump(i.frame!)}>{t("show")}</button>}
                {i.key === "chkLoud" && <button onClick={() => onCommit({ ...project, loudness: -14 })}>{t("fixIt")}</button>}
              </li>
            ))}
          </ul>
        ))}

      <h4>{t("listing")}</h4>
      <p className="hint small">{t("listingHint")}</p>
      {field("kind", "lstKind")}
      {field("place", "lstPlace")}
      <div className="row">
        {field("area", "lstArea")}
        {field("rooms", "lstRooms")}
      </div>
      {field("extra", "lstExtra", { area: true })}
      {field("price", "lstPrice")}
      {field("phone", "lstPhone", { ltr: true })}
      <button className="primary" onClick={() => onCommit({ ...project, texts: [...project.texts, ...listingTexts(project, listing, font, accent)] })}>
        {t("lstMake")}
      </button>
    </div>
  );
}
