import { useT, type TKey } from "./i18n";
import { movePreset, setTransformAt, speedOf, transformAt, type MovePreset } from "./motion";
import type { Adjust, Clip, FontInfo, Rgba, TextAnim, TextAnimKind, TextLayer, TransitionKind } from "./types";
import { IDENTITY, NEUTRAL, fileName } from "./types";

const ANIMS: TextAnimKind[] = ["none", "fade", "rise", "pop", "zoom", "words"];
const TRANSITIONS: ("none" | TransitionKind)[] = ["none", "dissolve", "black", "slide", "wipe", "zoom"];

/** Ready-made looks: our own simple recipes of the four colour controls. */
export const LOOKS: Record<string, Adjust> = {
  none: NEUTRAL,
  warm: { brightness: 0.03, contrast: 0.08, saturation: 0.1, temperature: 0.45 },
  cool: { brightness: 0, contrast: 0.05, saturation: 0, temperature: -0.45 },
  vivid: { brightness: 0.02, contrast: 0.18, saturation: 0.45, temperature: 0.05 },
  soft: { brightness: 0.06, contrast: -0.18, saturation: -0.15, temperature: 0.1 },
  mono: { brightness: 0, contrast: 0.15, saturation: -1, temperature: 0 },
  bright: { brightness: 0.15, contrast: 0.05, saturation: 0.1, temperature: 0 },
};
const sameAdjust = (a: Adjust, b: Adjust) => (Object.keys(a) as (keyof Adjust)[]).every((k) => Math.abs(a[k] - b[k]) < 1e-3);

function AnimPicker(props: { label: string; value?: TextAnim | null; fps: number; onChange: (a: TextAnim | null) => void }) {
  const t = useT();
  const kind = props.value?.kind ?? "none";
  const frames = props.value?.frames ?? Math.round(props.fps * 0.5);
  return (
    <div className="anim-picker">
      <span className="label">{props.label}</span>
      <div className="chips">
        {ANIMS.map((k) => (
          <button key={k} className={kind === k ? "on" : ""} onClick={() => props.onChange(k === "none" ? null : { kind: k, frames: k === "words" && kind !== "words" ? Math.round(props.fps * 1.2) : frames })}>
            {t(`anim_${k}` as TKey)}
          </button>
        ))}
      </div>
      {kind !== "none" && (
        <Slider label={t("animSeconds")} min={0.1} max={3} step={0.1} value={+(frames / props.fps).toFixed(1)} onChange={(v) => props.onChange({ kind: kind as TextAnimKind, frames: Math.max(1, Math.round(v * props.fps)) })} />
      )}
    </div>
  );
}

const hex = (c: number[]) => "#" + c.slice(0, 3).map((v) => v.toString(16).padStart(2, "0")).join("");
const fromHex = (s: string, a = 255): Rgba => [
  parseInt(s.slice(1, 3), 16),
  parseInt(s.slice(3, 5), 16),
  parseInt(s.slice(5, 7), 16),
  a,
];

function Slider(props: { label: string; min: number; max: number; step: number; value: number; onChange: (v: number) => void; format?: (v: number) => string }) {
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

export function TextProperties(props: { layer: TextLayer; fonts: FontInfo[]; swatches?: string[]; fps: number; onChange: (t: TextLayer) => void; onDelete: () => void }) {
  const t = useT();
  const { layer, fps } = props;
  const set = (patch: Partial<TextLayer>) => props.onChange({ ...layer, ...patch });
  const pct = (v: number) => `${Math.round(v * 100)}٪`;
  return (
    <div className="panel-body">
      <label>
        {t("textValue")}
        <textarea dir="auto" rows={3} value={layer.text} onChange={(e) => set({ text: e.target.value })} style={{ fontFamily: `"${layer.font_family}"` }} />
      </label>
      <label>
        {t("font")}
        <select value={layer.font_family} onChange={(e) => set({ font_family: e.target.value })}>
          {props.fonts.map((f) => (
            <option key={f.family} value={f.family}>
              {f.family}
            </option>
          ))}
        </select>
      </label>
      <Slider label={t("size")} min={0.015} max={0.25} step={0.001} value={layer.size} onChange={(v) => set({ size: v })} format={(v) => (v * 100).toFixed(1)} />
      <div className="row">
        <label className="narrow">
          {t("color")}
          <input type="color" value={hex(layer.color)} onChange={(e) => set({ color: fromHex(e.target.value) })} />
        </label>
        {props.swatches?.map((c) => (
          <button key={c} className="swatch" style={{ background: c }} aria-label={c} title={c} onClick={() => set({ color: fromHex(c) })} />
        ))}
        <label className="check">
          <input type="checkbox" checked={layer.weight >= 700} onChange={(e) => set({ weight: e.target.checked ? 700 : 400 })} />
          {t("bold")}
        </label>
        <label className="check">
          <input type="checkbox" checked={layer.shadow} onChange={(e) => set({ shadow: e.target.checked })} />
          {t("shadow")}
        </label>
      </div>
      <div className="row">
        <label className="check">
          <input type="checkbox" checked={layer.outline > 0} onChange={(e) => set({ outline: e.target.checked ? 0.05 : 0 })} />
          {t("outline")}
        </label>
        {layer.outline > 0 && (
          <input type="color" value={hex(layer.outline_color)} onChange={(e) => set({ outline_color: fromHex(e.target.value) })} />
        )}
        <label className="check">
          <input type="checkbox" checked={layer.background[3] > 0} onChange={(e) => set({ background: e.target.checked ? [0, 0, 0, 180] : [0, 0, 0, 0] })} />
          {t("background")}
        </label>
        {layer.background[3] > 0 && (
          <input type="color" value={hex(layer.background)} onChange={(e) => set({ background: fromHex(e.target.value, layer.background[3]) })} />
        )}
      </div>
      {layer.outline > 0 && (
        <Slider label={t("outline")} min={0.01} max={0.15} step={0.005} value={layer.outline} onChange={(v) => set({ outline: v })} format={(v) => (v * 100).toFixed(1)} />
      )}
      {layer.background[3] > 0 && (
        <Slider label={t("opacity")} min={20} max={255} step={1} value={layer.background[3]} onChange={(v) => set({ background: [layer.background[0], layer.background[1], layer.background[2], v] })} format={(v) => pct(v / 255)} />
      )}
      <div className="segmented">
        {(["right", "center", "left"] as const).map((a) => (
          <button key={a} className={layer.align === a ? "on" : ""} onClick={() => set({ align: a })}>
            {t(a)}
          </button>
        ))}
      </div>
      <Slider label={t("posX")} min={0} max={1} step={0.005} value={layer.x} onChange={(v) => set({ x: v })} format={pct} />
      <Slider label={t("posY")} min={0} max={1} step={0.005} value={layer.y} onChange={(v) => set({ y: v })} format={pct} />
      <Slider label={t("boxWidth")} min={0.2} max={1} step={0.01} value={layer.box_width} onChange={(v) => set({ box_width: v })} format={pct} />
      <label>
        {t("duration")}
        <input type="number" min={0.1} step={0.1} value={+(layer.duration / fps).toFixed(2)} onChange={(e) => set({ duration: Math.max(1, Math.round(+e.target.value * fps)) })} />
      </label>
      <AnimPicker label={t("animIn")} value={layer.anim_in} fps={fps} onChange={(a) => set({ anim_in: a })} />
      <AnimPicker label={t("animOut")} value={layer.anim_out} fps={fps} onChange={(a) => set({ anim_out: a })} />
      <button className="danger" onClick={props.onDelete}>
        {t("deleteText")}
      </button>
    </div>
  );
}

export function ClipProperties(props: {
  clip: Clip;
  fps: number;
  frame: number;
  /** Whether a clip ends exactly where this one starts on the same track. */
  hasBefore: boolean;
  onChange: (c: Clip) => void;
  onRipple: () => void;
  onLift: () => void;
}) {
  const t = useT();
  const { clip, fps, frame } = props;
  const set = (patch: Partial<Clip>) => props.onChange({ ...clip, ...patch });
  const moving = !!clip.keyframes?.length;
  const tr = transformAt(clip, Math.min(Math.max(frame, clip.start), clip.start + clip.duration - 1));
  const setT = (patch: Partial<Clip["transform"]>) => props.onChange(setTransformAt(clip, Math.min(Math.max(frame, clip.start), clip.start + clip.duration - 1), { ...tr, ...patch }));
  const secs = (f: number) => +(f / fps).toFixed(2);
  const pct = (v: number) => `${Math.round(v * 100)}٪`;
  const adjust = clip.adjust ?? NEUTRAL;
  const setA = (patch: Partial<Adjust>) => set({ adjust: { ...adjust, ...patch } });
  const speed = speedOf(clip);
  const setSpeed = (v: number) => {
    // Keep the same part of the file: faster means shorter on the timeline.
    const duration = Math.max(1, Math.round((clip.duration * speed) / v));
    set({ speed: v, duration, keyframes: clip.keyframes?.map((k) => ({ ...k, frame: Math.round((k.frame * speed) / v) })) });
  };
  const trKind = clip.transition_in?.kind ?? "none";
  const trFrames = clip.transition_in?.frames ?? Math.round(fps * 0.5);
  return (
    <div className="panel-body">
      <p dir="auto" className="filename">
        {fileName(clip.source)}
      </p>
      {clip.media !== "image" && (
        <>
          <Slider label={t("volume")} min={0} max={2} step={0.01} value={clip.volume} onChange={(v) => set({ volume: v })} format={pct} />
          <div className="row">
            <label>
              {t("fadeIn")}
              <input type="number" min={0} step={0.1} value={secs(clip.fade_in)} onChange={(e) => set({ fade_in: Math.min(clip.duration, Math.max(0, Math.round(+e.target.value * fps))) })} />
            </label>
            <label>
              {t("fadeOut")}
              <input type="number" min={0} step={0.1} value={secs(clip.fade_out)} onChange={(e) => set({ fade_out: Math.min(clip.duration, Math.max(0, Math.round(+e.target.value * fps))) })} />
            </label>
          </div>
          <span className="label">{t("speed")}</span>
          <div className="chips">
            {[0.25, 0.5, 1, 1.5, 2, 4].map((v) => (
              <button key={v} className={Math.abs(speed - v) < 1e-3 ? "on" : ""} onClick={() => setSpeed(v)} dir="ltr">
                {v}×
              </button>
            ))}
          </div>
        </>
      )}
      {clip.media !== "audio" && (
        <>
          <Slider label={t("scale")} min={0.05} max={3} step={0.01} value={tr.scale} onChange={(v) => setT({ scale: v })} format={pct} />
          <Slider label={t("posX")} min={-1} max={1} step={0.005} value={tr.x} onChange={(v) => setT({ x: v })} format={pct} />
          <Slider label={t("posY")} min={-1} max={1} step={0.005} value={tr.y} onChange={(v) => setT({ y: v })} format={pct} />
          <Slider label={t("opacity")} min={0} max={1} step={0.01} value={tr.opacity} onChange={(v) => setT({ opacity: v })} format={pct} />
          <button onClick={() => set({ transform: IDENTITY, keyframes: [] })}>{t("resetPosition")}</button>

          <span className="label">{t("motion")}</span>
          <div className="chips">
            {(["zoomIn", "zoomOut", "panRight", "panLeft"] as MovePreset[]).map((k) => (
              <button key={k} onClick={() => set({ keyframes: movePreset(clip, k) })}>
                {t(k)}
              </button>
            ))}
            {moving && (
              <button onClick={() => set({ transform: tr, keyframes: [] })}>{t("noMotion")}</button>
            )}
          </div>
          {moving && <p className="hint small">{t("motionHint")}</p>}

          <span className="label">{t("look")}</span>
          <div className="chips">
            {Object.entries(LOOKS).map(([k, v]) => (
              <button key={k} className={sameAdjust(adjust, v) ? "on" : ""} onClick={() => set({ adjust: v })}>
                {t(`look_${k}` as TKey)}
              </button>
            ))}
          </div>
          {(["brightness", "contrast", "saturation", "temperature"] as const).map((k) => (
            <Slider key={k} label={t(k)} min={-1} max={1} step={0.01} value={adjust[k]} onChange={(v) => setA({ [k]: v })} format={(v) => (v > 0 ? "+" : "") + Math.round(v * 100)} />
          ))}

          {props.hasBefore && (
            <>
              <span className="label">{t("transitionIn")}</span>
              <div className="chips">
                {TRANSITIONS.map((k) => (
                  <button key={k} className={trKind === k ? "on" : ""} onClick={() => set({ transition_in: k === "none" ? null : { kind: k, frames: trFrames } })}>
                    {t(`tr_${k}` as TKey)}
                  </button>
                ))}
              </div>
              {trKind !== "none" && (
                <Slider label={t("animSeconds")} min={0.1} max={2} step={0.1} value={+(trFrames / fps).toFixed(1)} onChange={(v) => set({ transition_in: { kind: trKind as TransitionKind, frames: Math.min(clip.duration, Math.max(1, Math.round(v * fps))) } })} />
              )}
            </>
          )}
        </>
      )}
      {clip.media === "image" && (
        <label>
          {t("duration")}
          <input type="number" min={0.1} step={0.1} value={secs(clip.duration)} onChange={(e) => set({ duration: Math.max(1, Math.round(+e.target.value * fps)) })} />
        </label>
      )}
      <div className="row">
        <button className="danger" onClick={props.onRipple}>
          {t("deleteClip")}
        </button>
        <button onClick={props.onLift}>{t("liftClip")}</button>
      </div>
    </div>
  );
}
