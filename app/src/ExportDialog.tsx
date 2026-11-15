import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, type ExportJob } from "./api";
import { useT } from "./i18n";
import type { Project } from "./types";
import { FORMATS } from "./types";

const QUALITY = [
  { key: "qBest", crf: 17 },
  { key: "qBalanced", crf: 21 },
  { key: "qSmall", crf: 27 },
] as const;

type State =
  | { step: "settings" }
  | { step: "running"; job: number; jobs: number; done: number; total: number }
  | { step: "finished"; ok: boolean; message: string };

const sameShape = (a: { width: number; height: number }, b: { width: number; height: number }) =>
  Math.abs(a.width * b.height - a.height * b.width) <= Math.max(a.width, a.height);

export function ExportDialog({ project, onClose }: { project: Project; onClose: () => void }) {
  const t = useT();
  const [crf, setCrf] = useState(21);
  const [scale, setScale] = useState(1);
  const [extra, setExtra] = useState<string[]>([]);
  const [fill, setFill] = useState(true);
  const [state, setState] = useState<State>({ step: "settings" });
  const others = FORMATS.filter((f) => !sameShape(f, project));

  useEffect(() => {
    const a = listen<{ job: number; jobs: number; done: number; total: number }>("export-progress", (e) =>
      setState({ step: "running", ...e.payload }),
    );
    const b = listen<{ ok: boolean; message: string }>("export-finished", (e) => setState({ step: "finished", ...e.payload }));
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  const even = (v: number) => Math.max(2, Math.round(v / 2) * 2);
  const start = async () => {
    const base = project.name || "video";
    const path = await saveDialog({ defaultPath: `${base}.mp4`, filters: [{ name: "MP4", extensions: ["mp4"] }] });
    if (!path) return;
    const stem = path.replace(/\.mp4$/i, "");
    const jobs: ExportJob[] = [{ path, width: even(project.width * scale), height: even(project.height * scale), crf }];
    for (const f of others.filter((f) => extra.includes(f.id))) {
      // Other shapes keep the same long side as the main version.
      const k = (Math.max(project.width, project.height) * scale) / Math.max(f.width, f.height);
      jobs.push({ path: `${stem} ${f.id.replace(":", "x")}.mp4`, width: even(f.width * k), height: even(f.height * k), crf, fill });
    }
    setState({ step: "running", job: 0, jobs: jobs.length, done: 0, total: 1 });
    api.startExport(jobs).catch((e) => setState({ step: "finished", ok: false, message: String(e) }));
  };

  return (
    <div className="modal-back">
      <div className="modal" role="dialog" aria-label={t("exportTitle")}>
        <h2>{t("exportTitle")}</h2>
        {state.step === "settings" && (
          <>
            <p className="label">{t("quality")}</p>
            <div className="segmented">
              {QUALITY.map((q) => (
                <button key={q.key} className={crf === q.crf ? "on" : ""} onClick={() => setCrf(q.crf)}>
                  {t(q.key)}
                </button>
              ))}
            </div>
            <p className="label">{t("resolution")}</p>
            <div className="segmented">
              {[1, 0.5].map((s) => (
                <button key={s} className={scale === s ? "on" : ""} onClick={() => setScale(s)}>
                  {t(s === 1 ? "full" : "half")}{" "}
                  <span dir="ltr" className="muted">
                    {even(project.width * s)}×{even(project.height * s)}
                  </span>
                </button>
              ))}
            </div>
            <p className="label">{t("otherSizes")}</p>
            <div className="chips">
              {others.map((f) => (
                <button key={f.id} className={extra.includes(f.id) ? "on" : ""} onClick={() => setExtra((x) => (x.includes(f.id) ? x.filter((y) => y !== f.id) : [...x, f.id]))}>
                  {t(f.label)} <span dir="ltr">{f.id}</span>
                </button>
              ))}
            </div>
            {extra.length > 0 && (
              <>
                <p className="hint small">{t("otherSizesHint")}</p>
                <label className="check">
                  <input type="checkbox" checked={fill} onChange={(e) => setFill(e.target.checked)} />
                  {t("fillFrame")}
                </label>
              </>
            )}
            <p className="muted" dir="ltr">
              MP4 · H.264 · AAC · {Math.round(project.fps.num / project.fps.den)} fps
            </p>
            <div className="modal-actions">
              <button className="primary" onClick={start}>
                {t("startExport")}
              </button>
              <button onClick={onClose}>{t("cancel")}</button>
            </div>
          </>
        )}
        {state.step === "running" && (
          <>
            <p>
              {t("exporting")}…{" "}
              {state.jobs > 1 && (
                <span className="muted">
                  {t("versionOf")} {state.job + 1} {t("of")} {state.jobs}
                </span>
              )}
            </p>
            <progress value={state.done} max={Math.max(1, state.total)} />
            <p className="muted">{Math.round((100 * state.done) / Math.max(1, state.total))}٪</p>
            <div className="modal-actions">
              <button onClick={() => api.cancelExport()}>{t("cancel")}</button>
            </div>
          </>
        )}
        {state.step === "finished" && (
          <>
            <p className={state.ok ? "ok" : "error"}>{t(state.ok ? "exportDone" : "exportFailed")}</p>
            <p className="muted export-files" dir="auto">
              {state.message}
            </p>
            <div className="modal-actions">
              <button className="primary" onClick={onClose}>
                {t("close")}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
