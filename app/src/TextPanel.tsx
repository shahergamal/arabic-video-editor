import type { TextLayer } from "./types";

interface Props {
  layer: TextLayer;
  fonts: string[];
  fps: number;
  onChange: (t: TextLayer) => void;
  onDelete: () => void;
}

const hex = (c: number[]) => "#" + c.slice(0, 3).map((v) => v.toString(16).padStart(2, "0")).join("");
const fromHex = (s: string): [number, number, number, number] => [
  parseInt(s.slice(1, 3), 16),
  parseInt(s.slice(3, 5), 16),
  parseInt(s.slice(5, 7), 16),
  255,
];

export function TextPanel({ layer, fonts, fps, onChange, onDelete }: Props) {
  const set = (patch: Partial<TextLayer>) => onChange({ ...layer, ...patch });
  return (
    <div className="panel-body">
      <label>
        النص
        <textarea dir="auto" rows={3} value={layer.text} onChange={(e) => set({ text: e.target.value })} />
      </label>
      <label>
        الخط
        <select value={layer.font_family} onChange={(e) => set({ font_family: e.target.value })}>
          {fonts.map((f) => (
            <option key={f} value={f} style={{ fontFamily: f }}>
              {f}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        <label>
          الحجم
          <input type="range" min={0.02} max={0.2} step={0.002} value={layer.size} onChange={(e) => set({ size: +e.target.value })} />
        </label>
        <label className="narrow">
          اللون
          <input type="color" value={hex(layer.color)} onChange={(e) => set({ color: fromHex(e.target.value) })} />
        </label>
      </div>
      <div className="row">
        <label className="check">
          <input type="checkbox" checked={layer.weight >= 700} onChange={(e) => set({ weight: e.target.checked ? 700 : 400 })} />
          عريض
        </label>
        <label className="check">
          <input type="checkbox" checked={layer.shadow} onChange={(e) => set({ shadow: e.target.checked })} />
          ظل
        </label>
      </div>
      <label>
        المكان من فوق لتحت
        <input type="range" min={0.05} max={0.95} step={0.005} value={layer.y} onChange={(e) => set({ y: +e.target.value })} />
      </label>
      <label>
        المدة (ثانية)
        <input
          type="number"
          min={0.1}
          step={0.1}
          value={+(layer.duration / fps).toFixed(2)}
          onChange={(e) => set({ duration: Math.max(1, Math.round(+e.target.value * fps)) })}
        />
      </label>
      <button className="danger" onClick={onDelete}>
        حذف النص
      </button>
    </div>
  );
}
