import { useT } from "./i18n";
import type { Clip, FontInfo, Rgba, TextLayer } from "./types";
import { IDENTITY, fileName } from "./types";

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

export function TextProperties(props: { layer: TextLayer; fonts: FontInfo[]; fps: number; onChange: (t: TextLayer) => void; onDelete: () => void }) {
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
      <button className="danger" onClick={props.onDelete}>
        {t("deleteText")}
      </button>
    </div>
  );
}

export function ClipProperties(props: { clip: Clip; fps: number; onChange: (c: Clip) => void; onRipple: () => void; onLift: () => void }) {
  const t = useT();
  const { clip, fps } = props;
  const set = (patch: Partial<Clip>) => props.onChange({ ...clip, ...patch });
  const tr = clip.transform;
  const setT = (patch: Partial<Clip["transform"]>) => set({ transform: { ...tr, ...patch } });
  const secs = (f: number) => +(f / fps).toFixed(2);
  const pct = (v: number) => `${Math.round(v * 100)}٪`;
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
        </>
      )}
      {clip.media !== "audio" && (
        <>
          <Slider label={t("scale")} min={0.05} max={3} step={0.01} value={tr.scale} onChange={(v) => setT({ scale: v })} format={pct} />
          <Slider label={t("posX")} min={-1} max={1} step={0.005} value={tr.x} onChange={(v) => setT({ x: v })} format={pct} />
          <Slider label={t("posY")} min={-1} max={1} step={0.005} value={tr.y} onChange={(v) => setT({ y: v })} format={pct} />
          <Slider label={t("opacity")} min={0} max={1} step={0.01} value={tr.opacity} onChange={(v) => setT({ opacity: v })} format={pct} />
          <button onClick={() => set({ transform: IDENTITY })}>{t("resetPosition")}</button>
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
