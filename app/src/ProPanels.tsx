// Phase 3 controls for a clip: background removal, green screen, masks with
// tracking, speed ramps and sound clean-up.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { useT } from "./i18n";
import type { Clip, Mask, SpeedPoint } from "./types";

export function Slider(props: { label: string; min: number; max: number; step: number; value: number; onChange: (v: number) => void; format?: (v: number) => string }) {
  return (
    <label>
      <span className="label-row">
        {props.label}
        <span className="value">{props.format ? props.format(props.value) : props.value}</span>
      </span>
      <input type="range" min={props.min} max={props.max} step={props.step} value={props.value} onChange={(e) => props.onChange(+e.target.value)} />
    </label>
  );
}

const pct = (v: number) => `${Math.round(v * 100)}٪`;
type Busy = { done: number; total: number } | null;

/** Listens to "<name>-progress" / "<name>-finished" while mounted. */
function useJob<T>(name: string, onDone: (ok: boolean, message: string, result: T | null) => void) {
  const [busy, setBusy] = useState<Busy>(null);
  useEffect(() => {
    const a = listen<{ done: number; total: number }>(`${name}-progress`, (e) => setBusy(e.payload));
    const b = listen<{ ok: boolean; message: string; result: T | null }>(`${name}-finished`, (e) => {
      setBusy(null);
      onDone(e.payload.ok, e.payload.message, e.payload.result);
    });
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name]);
  return [busy, setBusy] as const;
}

function Progress({ busy, label }: { busy: Busy; label: string }) {
  const t = useT();
  if (!busy) return null;
  const p = Math.round((100 * busy.done) / Math.max(1, busy.total));
  return (
    <>
      <p className="hint small">
        {label}… {p}٪
      </p>
      <progress value={p} max={100} />
      <button onClick={() => api.cancelExport()}>{t("cancel")}</button>
    </>
  );
}

export function CutoutSection({ clip, onChange }: { clip: Clip; onChange: (c: Clip) => void }) {
  const t = useT();
  const [models, setModels] = useState<{ id: string; mb: number; downloaded: boolean }[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const refresh = () => api.matteModels().then(setModels).catch(() => {});
  useEffect(() => {
    refresh();
  }, []);
  const [dl, setDl] = useJob<null>("model", (ok, msg) => {
    if (!ok && msg !== "cancelled") setMessage(`${t("downloadFailed")} (${msg})`);
    refresh();
  });
  const [prep, setPrep] = useJob<null>("matte", (ok, msg) => {
    if (ok) setReady(true);
    else if (msg !== "cancelled") setMessage(msg);
  });
  const model = clip.remove_bg ?? null;
  const info = models.find((m) => m.id === model);
  const key = clip.chroma;
  const hex = key ? "#" + key.color.map((v) => v.toString(16).padStart(2, "0")).join("") : "#00d040";
  return (
    <>
      <span className="label">{t("removeBg")}</span>
      <div className="chips">
        {([null, "people", "objects"] as const).map((m) => (
          <button
            key={m ?? "off"}
            className={model === m ? "on" : ""}
            onClick={() => {
              setReady(false);
              onChange({ ...clip, remove_bg: m });
            }}
          >
            {t(m === null ? "bgOff" : m === "people" ? "bgPeople" : "bgObjects")}
          </button>
        ))}
      </div>
      {model && <p className="hint small">{t("bgHint")}</p>}
      {model && info && !info.downloaded && !dl && (
        <>
          <p className="hint small">
            {t("modelNeededShort")} ({info.mb} MB)
          </p>
          <button
            className="primary"
            onClick={() => {
              setMessage(null);
              setDl({ done: 0, total: 1 });
              api.downloadMatteModel(model).catch((e) => {
                setDl(null);
                setMessage(String(e));
              });
            }}
          >
            {t("download")}
          </button>
        </>
      )}
      <Progress busy={dl} label={t("downloading")} />
      {model && info?.downloaded && !prep && (
        <button
          onClick={() => {
            setMessage(null);
            setPrep({ done: 0, total: clip.duration });
            api.prepareMatte(clip.id).catch((e) => {
              setPrep(null);
              setMessage(String(e));
            });
          }}
        >
          {ready ? `✓ ${t("prepared")}` : t("prepare")}
        </button>
      )}
      <Progress busy={prep} label={t("preparing")} />

      <label className="check">
        <input
          type="checkbox"
          checked={!!key}
          onChange={(e) => onChange({ ...clip, chroma: e.target.checked ? { color: [0, 208, 64], strength: 0.3, softness: 0.2 } : null })}
        />
        {t("greenScreen")}
      </label>
      {key && (
        <>
          <label className="row">
            <input
              type="color"
              value={hex}
              onChange={(e) => {
                const v = e.target.value;
                onChange({ ...clip, chroma: { ...key, color: [1, 3, 5].map((i) => parseInt(v.slice(i, i + 2), 16)) as [number, number, number] } });
              }}
            />
            {t("keyColor")}
          </label>
          <Slider label={t("keyStrength")} min={0} max={1} step={0.01} value={key.strength} onChange={(v) => onChange({ ...clip, chroma: { ...key, strength: v } })} format={pct} />
          <Slider label={t("keySoftness")} min={0} max={1} step={0.01} value={key.softness} onChange={(v) => onChange({ ...clip, chroma: { ...key, softness: v } })} format={pct} />
        </>
      )}
      {message && (
        <p className="hint small" dir="auto">
          {message}
        </p>
      )}
    </>
  );
}

const NEW_MASK: Mask = { shape: "ellipse", mode: "blur", x: 0.5, y: 0.5, w: 0.25, h: 0.2, feather: 0.2, blur: 0.03 };

export function MaskSection({ clip, frame, onChange }: { clip: Clip; frame: number; onChange: (c: Clip) => void }) {
  const t = useT();
  const m = clip.mask;
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useJob<{ frame: number; x: number; y: number }[]>("track", (ok, msg, path) => {
    if (ok && path && clip.mask) onChange({ ...clip, mask: { ...clip.mask, path } });
    else if (!ok && msg !== "cancelled") setMessage(msg);
  });
  const set = (patch: Partial<Mask>) => m && onChange({ ...clip, mask: { ...m, ...patch } });
  const local = Math.min(Math.max(0, frame - clip.start), clip.duration - 1);
  return (
    <>
      <div className="chips">
        {(["off", "blur", "keep", "cut"] as const).map((k) => (
          <button
            key={k}
            className={(m?.mode ?? "off") === k ? "on" : ""}
            onClick={() => onChange({ ...clip, mask: k === "off" ? null : { ...(m ?? NEW_MASK), mode: k } })}
          >
            {t(k === "off" ? "maskOff" : k === "blur" ? "maskBlur" : k === "keep" ? "maskKeep" : "maskCut")}
          </button>
        ))}
      </div>
      {m && (
        <>
          <div className="segmented">
            {(["ellipse", "rect"] as const).map((s) => (
              <button key={s} className={m.shape === s ? "on" : ""} onClick={() => set({ shape: s })}>
                {t(s === "rect" ? "shapeRect" : "shapeEllipse")}
              </button>
            ))}
          </div>
          {!m.path?.length && (
            <>
              <Slider label={t("posX")} min={0} max={1} step={0.005} value={m.x} onChange={(v) => set({ x: v })} format={pct} />
              <Slider label={t("posY")} min={0} max={1} step={0.005} value={m.y} onChange={(v) => set({ y: v })} format={pct} />
            </>
          )}
          <Slider label={t("maskW")} min={0.02} max={1} step={0.005} value={m.w} onChange={(v) => set({ w: v })} format={pct} />
          <Slider label={t("maskH")} min={0.02} max={1} step={0.005} value={m.h} onChange={(v) => set({ h: v })} format={pct} />
          <Slider label={t("feather")} min={0} max={1} step={0.01} value={m.feather} onChange={(v) => set({ feather: v })} format={pct} />
          {m.mode === "blur" && <Slider label={t("blurAmount")} min={0.005} max={0.08} step={0.001} value={m.blur} onChange={(v) => set({ blur: v })} format={(v) => (v * 100).toFixed(1)} />}
          {clip.media === "video" && !busy && (
            <div className="row">
              <button
                onClick={() => {
                  setMessage(null);
                  setBusy({ done: 0, total: clip.duration - local });
                  api.trackMask(clip.id, local, m.x, m.y, m.w, m.h).catch((e) => {
                    setBusy(null);
                    setMessage(String(e));
                  });
                }}
              >
                {t("trackIt")}
              </button>
              {!!m.path?.length && <button onClick={() => set({ path: [] })}>{t("clearTrack")}</button>}
            </div>
          )}
          {!!m.path?.length && <p className="hint small">✓ {t("tracked")}</p>}
          <Progress busy={busy} label={t("tracking")} />
          {message && (
            <p className="hint small" dir="auto">
              {message}
            </p>
          )}
        </>
      )}
    </>
  );
}

/** Frames of the source file a clip uses up (its speed, or its ramp, summed). */
export function sourceFrames(clip: Clip): number {
  const r = clip.speed_ramp ?? [];
  if (!r.length) return clip.duration * (clip.speed ?? 1);
  let sum = 0;
  for (let f = 0; f < clip.duration; f++) {
    let v = r[r.length - 1].speed;
    if (f <= r[0].frame) v = r[0].speed;
    else
      for (let j = 0; j + 1 < r.length; j++)
        if (f <= r[j + 1].frame) {
          v = r[j].speed + ((r[j + 1].speed - r[j].speed) * (f - r[j].frame)) / Math.max(1, r[j + 1].frame - r[j].frame);
          break;
        }
    sum += v;
  }
  return sum;
}

export type RampKind = "none" | "up" | "down" | "middle";
/** Shapes as (position 0..1, speed) pairs. */
const SHAPES: Record<Exclude<RampKind, "none">, [number, number][]> = {
  up: [
    [0, 1],
    [1, 3],
  ],
  down: [
    [0, 3],
    [1, 1],
  ],
  middle: [
    [0, 2.5],
    [0.4, 0.35],
    [0.6, 0.35],
    [1, 2.5],
  ],
};
const MEAN: Record<Exclude<RampKind, "none">, number> = { up: 2, down: 2, middle: 0.4 * 1.425 * 2 + 0.2 * 0.35 };

/** A new ramp that covers the same part of the file the clip shows now. */
export function applyRamp(clip: Clip, kind: RampKind): Partial<Clip> {
  const used = sourceFrames(clip);
  const base = clip.speed ?? 1;
  if (kind === "none") return { speed_ramp: [], duration: Math.max(1, Math.round(used / base)), keyframes: [] };
  const duration = Math.max(2, Math.round(used / (MEAN[kind] * base)));
  const end = duration - 1;
  const speed_ramp: SpeedPoint[] = SHAPES[kind].map(([p, v]) => ({ frame: Math.round(p * end), speed: v * base }));
  return { speed_ramp, duration, keyframes: [] };
}

export function rampKind(clip: Clip): RampKind {
  const r = clip.speed_ramp ?? [];
  if (!r.length) return "none";
  if (r.length === 4) return "middle";
  return r[0].speed < r[r.length - 1].speed ? "up" : "down";
}
