import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { useT } from "./i18n";
import type { Project } from "./types";

const QUALITY = [
  { key: "qBest", crf: 17 },
  { key: "qBalanced", crf: 21 },
  { key: "qSmall", crf: 27 },
] as const;

type State =
  | { step: "settings" }
  | { step: "running"; done: number; total: number }
  | { step: "finished"; ok: boolean; message: string };

export function ExportDialog({ project, onClose }: { project: Project; onClose: () => void }) {
  const t = useT();
  const [crf, setCrf] = useState(21);
  const [scale, setScale] = useState(1);
  const [state, setState] = useState<State>({ step: "settings" });

  useEffect(() => {
    const a = listen<{ done: number; total: number }>("export-progress", (e) =>
      setState({ step: "running", ...e.payload }),
    );
    const b = listen<{ ok: boolean; message: string }>("export-finished", (e) =>
      setState({ step: "finished", ...e.payload }),
    );
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  const start = async () => {
    const path = await saveDialog({
      defaultPath: `${project.name || "video"}.mp4`,
      filters: [{ name: "MP4", extensions: ["mp4"] }],
    });
    if (!path) return;
    setState({ step: "running", done: 0, total: 1 });
    api.startExport(path, scale, crf).catch((e) => setState({ step: "finished", ok: false, message: String(e) }));
  };

  const even = (v: number) => Math.round((v * scale) / 2) * 2;
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
                    {Math.round((project.width * s) / 2) * 2}×{Math.round((project.height * s) / 2) * 2}
                  </span>
                </button>
              ))}
            </div>
            <p className="muted" dir="ltr">
              MP4 · H.264 · AAC · {even(project.width)}×{even(project.height)} · {Math.round(project.fps.num / project.fps.den)} fps
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
            <p>{t("exporting")}…</p>
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
            <p className="muted" dir="auto">
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
