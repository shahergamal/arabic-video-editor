import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { CAPTION_PREFIX, segmentsToTexts, type Segment } from "./captions";
import { api5 } from "./api5";
import { useT } from "./i18n";
import { PRESETS } from "./TextPresets";
import type { Project, TextLayer } from "./types";
import { fpsValue } from "./types";

type Model = { id: string; mb: number; downloaded: boolean };
type Busy = { what: "download" | "listen"; done: number; total: number } | null;

const STYLES = ["presetCaption", "presetLower", "presetSubtitle"] as const;

export function CaptionsPanel({ project, families, onAdd }: { project: Project; families: string[]; onAdd: (p: Project) => void }) {
  const t = useT();
  const [models, setModels] = useState<Model[]>([]);
  const [model, setModel] = useState("accurate");
  const [lang, setLang] = useState("ar");
  const [style, setStyle] = useState<(typeof STYLES)[number]>("presetCaption");
  const [replace, setReplace] = useState(true);
  const [translate, setTranslate] = useState(false);
  const [busy, setBusy] = useState<Busy>(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = () => api.captionModels().then(setModels).catch(() => {});
  useEffect(() => {
    refresh();
  }, []);

  // The result arrives later; keep the latest project and choices for it.
  const [pending, setPending] = useState<Segment[] | null>(null);
  useEffect(() => {
    const subs = [
      listen<{ done: number; total: number }>("model-progress", (e) => setBusy({ what: "download", ...e.payload })),
      listen<{ ok: boolean; message: string }>("model-finished", (e) => {
        setBusy(null);
        if (!e.payload.ok && e.payload.message !== "cancelled") setMessage(`${t("downloadFailed")} (${e.payload.message})`);
        refresh();
      }),
      listen<{ done: number; total: number }>("caption-progress", (e) => setBusy({ what: "listen", ...e.payload })),
      listen<{ ok: boolean; message: string; result: Segment[] | null }>("caption-finished", (e) => {
        setBusy(null);
        if (!e.payload.ok) setMessage(e.payload.message);
        else setPending(e.payload.result ?? []);
      }),
    ];
    return () => subs.forEach((s) => s.then((f) => f()));
  }, []);

  useEffect(() => {
    if (!pending) return;
    setPending(null);
    if (!pending.length) {
      setMessage(t("noSpeech"));
      return;
    }
    const preset = PRESETS.find((p) => p.key === style)!;
    const { key: _k, fontPref, text: _t, ...rest } = preset;
    const font_family = fontPref.find((f) => families.includes(f)) ?? families[0] ?? "Cairo";
    const lines = segmentsToTexts(pending, fpsValue(project.fps), { ...rest, font_family } as Omit<TextLayer, "id" | "start" | "duration" | "text">);
    const kept = replace ? project.texts.filter((x) => !x.id.startsWith(CAPTION_PREFIX)) : project.texts;
    onAdd({ ...project, texts: [...kept, ...lines] });
    setMessage(`${t("captionsDone")}: ${lines.length}`);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending]);

  const chosen = models.find((m) => m.id === model);
  const pct = busy ? Math.round((100 * busy.done) / Math.max(1, busy.total)) : 0;
  return (
    <div className="panel-body captions">
      <h3>{t("captionsTitle")}</h3>
      <p className="hint small">{t("captionsHint")}</p>

      {models.map((m) => (
        <button key={m.id} className={`model ${model === m.id ? "on" : ""}`} onClick={() => setModel(m.id)} disabled={!!busy}>
          <span>{t(m.id === "fast" ? "modelFast" : "modelAccurate")}</span>
          <small dir="ltr">{m.downloaded ? "✓" : `${m.mb} MB`}</small>
        </button>
      ))}

      <label>
        {t("spokenLanguage")}
        <select value={lang} onChange={(e) => setLang(e.target.value)}>
          <option value="ar">{t("langAr")}</option>
          <option value="en">{t("langEn")}</option>
          <option value="auto">{t("langAuto")}</option>
        </select>
      </label>
      <label>
        {t("captionStyle")}
        <select value={style} onChange={(e) => setStyle(e.target.value as (typeof STYLES)[number])}>
          {STYLES.map((s) => (
            <option key={s} value={s}>
              {t(s)}
            </option>
          ))}
        </select>
      </label>
      <label className="check">
        <input type="checkbox" checked={replace} onChange={(e) => setReplace(e.target.checked)} />
        {t("replaceCaptions")}
      </label>
      <label className="check">
        <input type="checkbox" checked={translate} onChange={(e) => setTranslate(e.target.checked)} />
        {t("translateToEnglish")}
      </label>

      {busy ? (
        <>
          <p>
            {t(busy.what === "download" ? "downloading" : "listening")}… {pct}٪
          </p>
          <progress value={pct} max={100} />
          <button onClick={() => api.cancelExport()}>{t("cancel")}</button>
        </>
      ) : chosen && !chosen.downloaded ? (
        <>
          <p className="hint small">
            {t("modelNeeded")} ({chosen.mb} MB)
          </p>
          <button
            className="primary"
            onClick={() => {
              setMessage(null);
              setBusy({ what: "download", done: 0, total: 1 });
              api.downloadModel(model).catch((e) => {
                setBusy(null);
                setMessage(String(e));
              });
            }}
          >
            {t("download")}
          </button>
        </>
      ) : (
        <button
          className="primary"
          disabled={!chosen}
          onClick={() => {
            setMessage(null);
            setBusy({ what: "listen", done: 0, total: 100 });
            api5.transcribe(model, lang, translate).catch((e) => {
              setBusy(null);
              setMessage(String(e));
            });
          }}
        >
          {t("makeCaptions")}
        </button>
      )}
      {message && (
        <p className="hint" dir="auto">
          {message}
        </p>
      )}
    </div>
  );
}
