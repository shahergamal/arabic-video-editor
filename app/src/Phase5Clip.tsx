// Phase 5 clip controls: turning, flipping and cropping the picture, blend
// modes, playing backwards, steadying, colour curves and wheels.
import { useEffect, useRef, useState } from "react";
import { api5 } from "./api5";
import { mediaInfo } from "./api";
import { setReverse } from "./edits5";
import { useT, type TKey } from "./i18n";
import { setTransformAt, transformAt } from "./motion";
import { Progress, Slider, useJob } from "./ProPanels";
import type { Adjust, BlendMode, Clip, CurvePoint, Curves } from "./types";

const BLENDS: BlendMode[] = ["normal", "multiply", "screen", "overlay", "add", "darken", "lighten"];
const pct = (v: number) => `${Math.round(v * 100)}٪`;
const deg = (v: number) => `${Math.round(v)}°`;

/** Rotation, mirror, crop and blend. */
export function FrameSection({ clip, frame, onChange }: { clip: Clip; frame: number; onChange: (c: Clip) => void }) {
  const t = useT();
  const at = Math.min(Math.max(frame, clip.start), clip.start + clip.duration - 1);
  const tr = transformAt(clip, at);
  const crop = clip.crop ?? { left: 0, top: 0, right: 0, bottom: 0 };
  const setCrop = (k: keyof typeof crop, v: number) => onChange({ ...clip, crop: { ...crop, [k]: Math.min(v, 0.95 - crop[k === "left" ? "right" : k === "right" ? "left" : k === "top" ? "bottom" : "top"]) } });
  return (
    <>
      <Slider label={t("rotation")} min={-180} max={180} step={1} value={tr.rotation ?? 0} onChange={(v) => onChange(setTransformAt(clip, at, { ...tr, rotation: v }))} format={deg} />
      <div className="chips">
        <button onClick={() => onChange(setTransformAt(clip, at, { ...tr, rotation: (((tr.rotation ?? 0) + 90 + 180) % 360) - 180 }))}>↻ 90°</button>
        <button className={clip.flip_h ? "on" : ""} onClick={() => onChange({ ...clip, flip_h: !clip.flip_h })}>
          ⇋ {t("flipH")}
        </button>
        <button className={clip.flip_v ? "on" : ""} onClick={() => onChange({ ...clip, flip_v: !clip.flip_v })}>
          ⇵ {t("flipV")}
        </button>
      </div>
      <span className="label">{t("crop")}</span>
      {(["left", "right", "top", "bottom"] as const).map((k) => (
        <Slider key={k} label={t(`crop_${k}` as TKey)} min={0} max={0.9} step={0.005} value={crop[k]} onChange={(v) => setCrop(k, v)} format={pct} />
      ))}
      {(crop.left || crop.right || crop.top || crop.bottom) > 0 && <button onClick={() => onChange({ ...clip, crop: undefined })}>{t("noCrop")}</button>}
      <label>
        {t("blend")}
        <select value={clip.blend || "normal"} onChange={(e) => onChange({ ...clip, blend: e.target.value === "normal" ? "" : (e.target.value as BlendMode) })}>
          {BLENDS.map((b) => (
            <option key={b} value={b}>
              {t(`blend_${b}` as TKey)}
            </option>
          ))}
        </select>
      </label>
    </>
  );
}

/** Backwards, frozen and steadied video. Both need a working copy of the file. */
export function TimeSection({ clip, fps, onChange }: { clip: Clip; fps: number; onChange: (c: Clip) => void }) {
  const t = useT();
  const [message, setMessage] = useState<string | null>(null);
  const [ready, setReady] = useState(true);
  const [busy, setBusy] = useJob<null>("copy", (ok, msg) => {
    setMessage(ok ? null : msg);
    setReady(ok);
  });
  const needs = !!clip.reverse || (clip.stabilize ?? 0) > 0;
  // A new copy is needed whenever reverse or steadying change.
  useEffect(() => {
    if (!needs) {
      setReady(true);
      return;
    }
    let live = true;
    const id = setTimeout(() => {
      api5.copyReady(clip.id).then((r) => {
        if (!live) return;
        setReady(r);
        if (!r) {
          setBusy({ done: 0, total: 1000 });
          api5.prepareCopy(clip.id).catch((e) => setMessage(String(e)));
        }
      });
    }, 600);
    return () => {
      live = false;
      clearTimeout(id);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [clip.id, clip.reverse, clip.stabilize, clip.source]);

  const toggleReverse = async () => {
    const info = await mediaInfo(clip.source);
    onChange(setReverse(clip, !clip.reverse, info.duration, fps));
  };
  return (
    <>
      {clip.freeze ? (
        <p className="hint small">{t("frozenHint")}</p>
      ) : (
        <label className="check">
          <input type="checkbox" checked={!!clip.reverse} onChange={toggleReverse} />
          {t("reverse")}
        </label>
      )}
      <Slider label={t("stabilize")} min={0} max={1} step={0.05} value={clip.stabilize ?? 0} onChange={(v) => onChange({ ...clip, stabilize: v })} format={(v) => (v ? pct(v) : t("off"))} />
      {needs && !ready && !busy && <p className="hint small">{t("copyWaiting")}</p>}
      <Progress busy={busy} label={t("preparingCopy")} />
      {message && <p className="hint small error">{message}</p>}
      <p className="hint small">{t("copyHint")}</p>
    </>
  );
}

// ---------- curves ----------

const CHANNELS = ["master", "r", "g", "b"] as const;
type Channel = (typeof CHANNELS)[number];
const CH_COLOR: Record<Channel, string> = { master: "#e8e8e8", r: "#ff6b6b", g: "#5fd35f", b: "#5aa0ff" };

/** Monotone cubic through the points, like the engine (for drawing only). */
export function curveAt(points: CurvePoint[], x: number): number {
  const p = [...points].sort((a, b) => a[0] - b[0]);
  if (!p.length || p[0][0] > 0) p.unshift([0, 0]);
  if (p[p.length - 1][0] < 1) p.push([1, 1]);
  const n = p.length;
  const d = p.slice(0, -1).map((q, i) => (p[i + 1][1] - q[1]) / Math.max(1e-6, p[i + 1][0] - q[0]));
  const m = p.map((_, i) => (i === 0 ? d[0] : i === n - 1 ? d[n - 2] : d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2));
  for (let i = 0; i < n - 1; i++) {
    if (d[i] === 0) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const a = m[i] / d[i];
    const b = m[i + 1] / d[i];
    const s = a * a + b * b;
    if (s > 9) {
      const k = 3 / Math.sqrt(s);
      m[i] = k * a * d[i];
      m[i + 1] = k * b * d[i];
    }
  }
  let i = 0;
  while (i < n - 2 && x > p[i + 1][0]) i++;
  const h = p[i + 1][0] - p[i][0] || 1e-6;
  const u = (x - p[i][0]) / h;
  const h00 = 2 * u ** 3 - 3 * u ** 2 + 1;
  const h10 = u ** 3 - 2 * u ** 2 + u;
  const h01 = -2 * u ** 3 + 3 * u ** 2;
  const h11 = u ** 3 - u ** 2;
  return Math.min(1, Math.max(0, h00 * p[i][1] + h10 * h * m[i] + h01 * p[i + 1][1] + h11 * h * m[i + 1]));
}

const SIZE = 220;

export function CurvesEditor({ curves, onChange }: { curves: Curves; onChange: (c: Curves) => void }) {
  const t = useT();
  const [ch, setCh] = useState<Channel>("master");
  const [drag, setDrag] = useState<number | null>(null);
  const box = useRef<SVGSVGElement>(null);
  const pts: CurvePoint[] = curves[ch]?.length ? curves[ch]! : [[0, 0], [1, 1]];
  const set = (p: CurvePoint[]) => onChange({ ...curves, [ch]: p });
  const pos = (e: { clientX: number; clientY: number }): CurvePoint => {
    const r = box.current!.getBoundingClientRect();
    return [Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height))];
  };
  const path = Array.from({ length: 65 }, (_, i) => {
    const x = i / 64;
    return `${i ? "L" : "M"}${(x * SIZE).toFixed(1)},${((1 - curveAt(pts, x)) * SIZE).toFixed(1)}`;
  }).join(" ");
  return (
    <div className="curves">
      <div className="chips">
        {CHANNELS.map((c) => (
          <button key={c} className={ch === c ? "on" : ""} style={{ color: CH_COLOR[c] }} onClick={() => setCh(c)}>
            {t(`ch_${c}` as TKey)}
          </button>
        ))}
      </div>
      <svg
        ref={box}
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className="curve-box"
        role="img"
        aria-label={t("curves")}
        onPointerDown={(e) => {
          const p = pos(e);
          // Near an existing point: drag it; otherwise add one.
          const near = pts.findIndex((q) => Math.hypot(q[0] - p[0], q[1] - p[1]) < 0.06);
          (e.target as Element).setPointerCapture?.(e.pointerId);
          if (near >= 0) setDrag(near);
          else {
            const next = [...pts, p].sort((a, b) => a[0] - b[0]);
            set(next);
            setDrag(next.indexOf(p));
          }
        }}
        onPointerMove={(e) => {
          if (drag === null) return;
          const p = pos(e);
          const next = pts.map((q, i) => (i === drag ? p : q));
          set(next);
        }}
        onPointerUp={() => setDrag(null)}
        onDoubleClick={(e) => {
          const p = pos(e);
          const near = pts.findIndex((q) => Math.hypot(q[0] - p[0], q[1] - p[1]) < 0.06);
          if (near > 0 && near < pts.length - 1) set(pts.filter((_, i) => i !== near));
        }}
      >
        {[0.25, 0.5, 0.75].map((g) => (
          <g key={g} className="grid">
            <line x1={g * SIZE} y1={0} x2={g * SIZE} y2={SIZE} />
            <line x1={0} y1={g * SIZE} x2={SIZE} y2={g * SIZE} />
          </g>
        ))}
        <line className="diag" x1={0} y1={SIZE} x2={SIZE} y2={0} />
        <path d={path} fill="none" stroke={CH_COLOR[ch]} strokeWidth={2} />
        {pts.map((q, i) => (
          <circle key={i} cx={q[0] * SIZE} cy={(1 - q[1]) * SIZE} r={5} fill={CH_COLOR[ch]} />
        ))}
      </svg>
      <p className="hint small">{t("curvesHint")}</p>
      <button onClick={() => set([[0, 0], [1, 1]])}>{t("resetCurve")}</button>
    </div>
  );
}

// ---------- colour wheels ----------

/** One wheel: dragging the dot pushes the colour towards that hue. */
function Wheel({ label, value, onChange }: { label: string; value: [number, number, number]; onChange: (v: [number, number, number]) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [down, setDown] = useState(false);
  // The dot shows the tint (the RGB values minus their average).
  const avg = (value[0] + value[1] + value[2]) / 3;
  const [r, g, b] = value.map((v) => v - avg);
  const x = r - 0.5 * g - 0.5 * b;
  const y = (Math.sqrt(3) / 2) * (g - b);
  const R = 0.5;
  const move = (e: React.PointerEvent) => {
    const box = ref.current!.getBoundingClientRect();
    let dx = ((e.clientX - box.left) / box.width) * 2 - 1;
    let dy = -(((e.clientY - box.top) / box.height) * 2 - 1);
    const len = Math.hypot(dx, dy);
    if (len > 1) {
      dx /= len;
      dy /= len;
    }
    // Back from the wheel to RGB offsets that average zero.
    const tr = (2 / 3) * dx * R;
    const tg = (-dx / 3 + dy / Math.sqrt(3)) * R;
    const tb = (-dx / 3 - dy / Math.sqrt(3)) * R;
    onChange([avg + tr, avg + tg, avg + tb]);
  };
  return (
    <div className="wheel-wrap">
      <div
        ref={ref}
        className="wheel"
        dir="ltr"
        onPointerDown={(e) => {
          setDown(true);
          (e.target as Element).setPointerCapture?.(e.pointerId);
          move(e);
        }}
        onPointerMove={(e) => down && move(e)}
        onPointerUp={() => setDown(false)}
        onDoubleClick={() => onChange([avg, avg, avg])}
      >
        <span className="wheel-dot" style={{ left: `${50 + (x / R) * 50 * 0.75}%`, top: `${50 - (y / R) * 50 * 0.75}%` }} />
      </div>
      <span className="label">{label}</span>
      <input
        type="range"
        min={-1}
        max={1}
        step={0.01}
        value={avg}
        aria-label={label}
        onChange={(e) => {
          const d = +e.target.value - avg;
          onChange([value[0] + d, value[1] + d, value[2] + d]);
        }}
      />
    </div>
  );
}

export function WheelsSection({ adjust, onChange }: { adjust: Adjust; onChange: (a: Partial<Adjust>) => void }) {
  const t = useT();
  const zero: [number, number, number] = [0, 0, 0];
  return (
    <>
      <div className="wheels">
        <Wheel label={t("lift")} value={adjust.lift ?? zero} onChange={(v) => onChange({ lift: v })} />
        <Wheel label={t("gamma")} value={adjust.gamma ?? zero} onChange={(v) => onChange({ gamma: v })} />
        <Wheel label={t("gain")} value={adjust.gain ?? zero} onChange={(v) => onChange({ gain: v })} />
      </div>
      <p className="hint small">{t("wheelsHint")}</p>
      <button onClick={() => onChange({ lift: undefined, gamma: undefined, gain: undefined })}>{t("resetWheels")}</button>
    </>
  );
}

export const usesCurves = (a?: Adjust) => !!a?.curves && Object.values(a.curves).some((p: CurvePoint[] | undefined) => !!p && p.some(([x, y]) => Math.abs(x - y) > 1e-3));
export const usesWheels = (a?: Adjust) => [a?.lift, a?.gamma, a?.gain].some((v) => v && v.some((x) => Math.abs(x) > 1e-3));
