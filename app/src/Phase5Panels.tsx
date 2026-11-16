// Phase 5 panels: shapes and stickers, voice recording, the new-version
// notice, the automatic edit, cutting to the beat and following the subject.
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, mediaInfo } from "./api";
import { api5, type AiView, type Beats, type Brief, type Planner, type PlanResult, type ShotInfo, type Update } from "./api5";
import { cutToBeats } from "./edits5";
import { place, addTrack, makeClip } from "./edits";
import { useT, type TKey } from "./i18n";
import { Progress, Slider, useJob } from "./ProPanels";
import { AnimPicker, fromHex, hex } from "./Properties";
import type { Clip, Project, Shape, ShapeKind, TextAnimKind } from "./types";
import { AUDIO_EXT, IMAGE_EXT, VIDEO_EXT, allClips, fileName, fpsValue, newId, projectDuration } from "./types";

// ---------- shapes and stickers ----------

type ShapeRecipe = Omit<Shape, "id" | "start" | "duration">;
const base = (kind: ShapeKind, patch: Partial<ShapeRecipe> = {}): ShapeRecipe => ({
  kind,
  x: 0.5,
  y: 0.5,
  w: 0.3,
  h: 0.2,
  rotation: 0,
  fill: [255, 255, 255, 255],
  stroke: [0, 0, 0, 0],
  stroke_width: 0,
  radius: 0,
  shadow: false,
  ...patch,
});

const SHAPES: { key: TKey; icon: string; make: () => ShapeRecipe }[] = [
  { key: "shape_rect", icon: "▬", make: () => base("rect", { radius: 0.15 }) },
  { key: "shape_ellipse", icon: "●", make: () => base("ellipse", { w: 0.2, h: 0.2 }) },
  { key: "shape_line", icon: "━", make: () => base("line", { h: 0, stroke: [255, 255, 255, 255], stroke_width: 0.008, anim_in: { kind: "draw", frames: 12 } }) },
  { key: "shape_arrow", icon: "➜", make: () => base("arrow", { h: 0, stroke: [255, 214, 10, 255], stroke_width: 0.012, anim_in: { kind: "draw", frames: 12 } }) },
  { key: "shape_star", icon: "★", make: () => base("star", { w: 0.18, h: 0.18, fill: [255, 196, 0, 255] }) },
];

/** Our own simple sticker recipes, built from shapes. */
const STICKERS: { key: TKey; icon: string; make: () => ShapeRecipe }[] = [
  { key: "stickerCircle", icon: "◯", make: () => base("ellipse", { w: 0.25, h: 0.14, fill: [0, 0, 0, 0], stroke: [255, 59, 48, 255], stroke_width: 0.008, anim_in: { kind: "pop", frames: 10 } }) },
  { key: "stickerPointer", icon: "↘", make: () => base("arrow", { x: 0.4, y: 0.4, w: 0.25, h: 0, rotation: 35, stroke: [255, 59, 48, 255], stroke_width: 0.014, shadow: true, anim_in: { kind: "draw", frames: 10 } }) },
  { key: "stickerBadge", icon: "✸", make: () => base("star", { x: 0.82, y: 0.15, w: 0.22, h: 0.22, fill: [255, 59, 48, 255], shadow: true, anim_in: { kind: "pop", frames: 10 } }) },
  { key: "stickerBar", icon: "▭", make: () => base("rect", { y: 0.82, w: 0.9, h: 0.1, fill: [0, 0, 0, 170], radius: 0.3, anim_in: { kind: "rise", frames: 10 } }) },
  { key: "stickerUnderline", icon: "﹏", make: () => base("line", { y: 0.62, w: 0.5, h: 0, stroke: [255, 214, 10, 255], stroke_width: 0.01, anim_in: { kind: "draw", frames: 15 } }) },
];

export function ShapesPanel({ project, frame, onCommit, onSelect }: { project: Project; frame: number; onCommit: (p: Project) => void; onSelect: (id: string) => void }) {
  const t = useT();
  const add = (r: ShapeRecipe) => {
    const s: Shape = { ...r, id: newId(), start: frame, duration: Math.round(fpsValue(project.fps) * 3) };
    onCommit({ ...project, shapes: [...(project.shapes ?? []), s] });
    onSelect(s.id);
  };
  return (
    <div className="panel-body">
      <h4>{t("shapes")}</h4>
      <div className="shape-grid">
        {SHAPES.map((s) => (
          <button key={s.key} onClick={() => add(s.make())} title={t(s.key)}>
            <span className="shape-icon">{s.icon}</span>
            {t(s.key)}
          </button>
        ))}
      </div>
      <h4>{t("stickers")}</h4>
      <div className="shape-grid">
        {STICKERS.map((s) => (
          <button key={s.key} onClick={() => add(s.make())} title={t(s.key)}>
            <span className="shape-icon">{s.icon}</span>
            {t(s.key)}
          </button>
        ))}
      </div>
      <p className="hint small">{t("shapesHint")}</p>
    </div>
  );
}

const SHAPE_ANIMS: TextAnimKind[] = ["none", "fade", "rise", "pop", "zoom"];

export function ShapeProperties({ shape, fps, swatches, onChange, onDelete }: { shape: Shape; fps: number; swatches?: string[]; onChange: (s: Shape) => void; onDelete: () => void }) {
  const t = useT();
  const set = (p: Partial<Shape>) => onChange({ ...shape, ...p });
  const pct = (v: number) => `${Math.round(v * 100)}٪`;
  const line = shape.kind === "line" || shape.kind === "arrow";
  const kinds = line ? [...SHAPE_ANIMS, "draw" as const] : SHAPE_ANIMS;
  const secs = (f: number) => +(f / fps).toFixed(2);
  const color = (label: string, value: Shape["fill"], key: "fill" | "stroke") => (
    <label className="color-row">
      {label}
      <input type="color" value={hex(value)} onChange={(e) => set({ [key]: fromHex(e.target.value, value[3] || 255) })} />
      <input type="range" min={0} max={255} value={value[3]} aria-label={t("opacity")} onChange={(e) => set({ [key]: [value[0], value[1], value[2], +e.target.value] as Shape["fill"] })} />
    </label>
  );
  return (
    <div className="panel-body">
      <p className="filename">{t(`shape_${shape.kind}` as TKey)}</p>
      {!line && color(t("fill"), shape.fill, "fill")}
      {color(line ? t("color") : t("edge"), shape.stroke, "stroke")}
      {swatches && swatches.length > 0 && (
        <div className="chips">
          {swatches.map((c) => (
            <button key={c} className="swatch" style={{ background: c }} aria-label={c} onClick={() => set(line ? { stroke: fromHex(c) } : { fill: fromHex(c) })} />
          ))}
        </div>
      )}
      <Slider label={line ? t("thickness") : t("edgeWidth")} min={0} max={0.04} step={0.001} value={shape.stroke_width} onChange={(v) => set({ stroke_width: v })} format={(v) => (v * 1000).toFixed(0)} />
      {shape.kind === "rect" && <Slider label={t("corners")} min={0} max={0.5} step={0.01} value={shape.radius} onChange={(v) => set({ radius: v })} format={pct} />}
      <Slider label={t("posX")} min={0} max={1} step={0.005} value={shape.x} onChange={(v) => set({ x: v })} format={pct} />
      <Slider label={t("posY")} min={0} max={1} step={0.005} value={shape.y} onChange={(v) => set({ y: v })} format={pct} />
      <Slider label={line ? t("length") : t("width")} min={0.02} max={1.5} step={0.005} value={shape.w} onChange={(v) => set({ w: v })} format={pct} />
      {!line && <Slider label={t("height")} min={0.02} max={1.5} step={0.005} value={shape.h} onChange={(v) => set({ h: v })} format={pct} />}
      <Slider label={t("rotation")} min={-180} max={180} step={1} value={shape.rotation} onChange={(v) => set({ rotation: v })} format={(v) => `${Math.round(v)}°`} />
      <label className="check">
        <input type="checkbox" checked={shape.shadow} onChange={(e) => set({ shadow: e.target.checked })} />
        {t("shadow")}
      </label>
      <label>
        {t("duration")}
        <input type="number" min={0.1} step={0.1} value={secs(shape.duration)} onChange={(e) => set({ duration: Math.max(1, Math.round(+e.target.value * fps)) })} />
      </label>
      <AnimPicker label={t("animIn")} value={shape.anim_in} fps={fps} kinds={kinds} onChange={(a) => set({ anim_in: a })} />
      <AnimPicker label={t("animOut")} value={shape.anim_out} fps={fps} kinds={kinds} onChange={(a) => set({ anim_out: a })} />
      <button className="danger" onClick={onDelete}>
        {t("delete")}
      </button>
    </div>
  );
}

// ---------- voice recording ----------

/** Records from the microphone and puts the take on a free audio track at the playhead. */
export function VoiceRecorder({ project, frame, onCommit, onNotice }: { project: Project; frame: number; onCommit: (p: Project) => void; onNotice: (s: string) => void }) {
  const t = useT();
  const [rec, setRec] = useState<MediaRecorder | null>(null);
  const [secs, setSecs] = useState(0);
  const startAt = useRef(0);
  const chunks = useRef<Blob[]>([]);
  const at = useRef(0);
  // The take goes into the project as it is when recording stops.
  const latest = useRef(project);
  latest.current = project;
  useEffect(() => {
    if (!rec) return;
    const id = setInterval(() => setSecs((Date.now() - startAt.current) / 1000), 200);
    return () => clearInterval(id);
  }, [rec]);
  const start = async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
      const r = new MediaRecorder(stream);
      chunks.current = [];
      r.ondataavailable = (e) => e.data.size && chunks.current.push(e.data);
      r.onstop = async () => {
        stream.getTracks().forEach((x) => x.stop());
        try {
          const bytes = new Uint8Array(await new Blob(chunks.current).arrayBuffer());
          const path = await api5.saveRecording(bytes);
          const info = await mediaInfo(path);
          let p = latest.current;
          const startFrame = at.current;
          const fps = fpsValue(p.fps);
          const len = Math.max(1, Math.round(info.duration * fps));
          let track = p.tracks.find((x) => x.kind === "audio" && !x.locked && x.clips.every((c) => c.start + c.duration <= startFrame || c.start >= startFrame + len));
          if (!track) {
            p = addTrack(p, "audio");
            track = p.tracks.filter((x) => x.kind === "audio").pop()!;
          }
          onCommit(place(p, track.id, makeClip(p, path, "audio", info.duration, startFrame)));
          onNotice(t("recordingAdded"));
        } catch (e) {
          onNotice(`${t("recordingFailed")}: ${e}`);
        }
      };
      at.current = frame;
      startAt.current = Date.now();
      setSecs(0);
      r.start(250);
      setRec(r);
    } catch (e) {
      onNotice(`${t("micFailed")}: ${e}`);
    }
  };
  const stop = () => {
    rec?.stop();
    setRec(null);
  };
  return rec ? (
    <button className="danger recording" onClick={stop}>
      ■ {t("stopRecording")} <span dir="ltr">{secs.toFixed(1)}s</span>
    </button>
  ) : (
    <button onClick={start} title={t("recordHint")}>
      🎙 {t("recordVoice")}
    </button>
  );
}

// ---------- new version notice ----------

export function UpdateBanner() {
  const t = useT();
  const [u, setU] = useState<Update | null>(null);
  const [hidden, setHidden] = useState(false);
  useEffect(() => {
    api5.checkUpdate().then(setU).catch(() => {});
  }, []);
  if (!u || hidden) return null;
  return (
    <div className="banner info">
      <span>
        {t("updateReady")} <b dir="ltr">{u.version}</b>
      </span>
      <button className="primary" onClick={() => api5.openReleasePage(u.url)}>
        {t("download")}
      </button>
      <button onClick={() => setHidden(true)}>{t("later")}</button>
    </div>
  );
}

// ---------- AI settings ----------

export function AiSettings({ onChange }: { onChange?: (v: AiView) => void }) {
  const t = useT();
  const [v, setV] = useState<AiView | null>(null);
  const [key, setKey] = useState("");
  const [msg, setMsg] = useState<string | null>(null);
  const [dl, setDl] = useJob<null>("llm", (ok, m) => {
    setMsg(ok ? t("localReady") : m);
    load();
  });
  const load = () =>
    api5.aiSettings().then((x) => {
      setV(x);
      onChange?.(x);
    });
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!v) return null;
  const save = (planner: Planner, cap = v.monthly_cap, k?: string) =>
    api5
      .setAiSettings(planner, cap, k)
      .then(load)
      .catch((e) => setMsg(String(e)));
  return (
    <div className="ai-settings">
      <span className="label">{t("planner")}</span>
      {(["rules", "local", "claude"] as Planner[]).map((p) => (
        <label key={p} className="radio">
          <input type="radio" name="planner" checked={v.planner === p} onChange={() => save(p)} />
          <span>
            <b>{t(`planner_${p}` as TKey)}</b>
            <br />
            <span className="hint small">{t(`planner_${p}_hint` as TKey)}</span>
          </span>
        </label>
      ))}
      {v.planner === "local" && !v.local_ready && (
        <>
          <p className="hint small">
            {t("localDownload")} ({v.local_mb} MB)
          </p>
          {!dl && (
            <button
              onClick={() => {
                setDl({ done: 0, total: 1 });
                api5.downloadLocalAi().catch((e) => setMsg(String(e)));
              }}
            >
              {t("download")}
            </button>
          )}
          <Progress busy={dl} label={t("downloading")} />
        </>
      )}
      {v.planner === "claude" && (
        <>
          <label>
            {t("claudeKey")} {v.has_key && <span className="ok">✓</span>}
            <input type="password" dir="ltr" placeholder={v.has_key ? "••••••••" : "sk-ant-…"} value={key} onChange={(e) => setKey(e.target.value)} />
          </label>
          <button
            disabled={!key.trim()}
            onClick={() => {
              save("claude", v.monthly_cap, key.trim());
              setKey("");
            }}
          >
            {t("saveKey")}
          </button>
          <Slider label={t("monthlyCap")} min={0.5} max={20} step={0.5} value={v.monthly_cap} onChange={(c) => save("claude", c)} format={(c) => `$${c.toFixed(2)}`} />
          <p className="hint small">
            {t("spentThisMonth")}: <span dir="ltr">${v.spent.toFixed(3)}</span>
          </p>
          <p className="hint small">{t("claudePrivacy")}</p>
        </>
      )}
      {msg && <p className="hint small">{msg}</p>}
    </div>
  );
}

// ---------- automatic edit ----------

const SHAPES_OF = [
  { key: "reels", w: 1080, h: 1920 },
  { key: "youtube", w: 1920, h: 1080 },
  { key: "square", w: 1080, h: 1080 },
] as const;

export function AutoEditPanel({ font, onOpen, onNotice }: { font: string; onOpen: (p: Project) => void; onNotice: (s: string) => void }) {
  const t = useT();
  const [idea, setIdea] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [shape, setShape] = useState(0);
  const [seconds, setSeconds] = useState(0);
  const [pace, setPace] = useState<Brief["pace"]>("");
  const [shots, setShots] = useState<ShotInfo[] | null>(null);
  const [ai, setAi] = useState<AiView | null>(null);
  const [estimate, setEstimate] = useState<{ dollars: number; spent: number; cap: number } | null>(null);
  const [planning, setPlanning] = useState(false);
  const [settings, setSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [analyzing, setAnalyzing] = useJob<ShotInfo[]>("analyze", (ok, msg, result) => {
    if (ok && result) setShots(result);
    else setError(msg);
  });
  const brief = (): Brief => ({ idea, width: SHAPES_OF[shape].w, height: SHAPES_OF[shape].h, seconds, pace, font });

  useEffect(() => {
    const un = listen<{ ok: boolean; message: string; result: PlanResult | null }>("plan-finished", (e) => {
      setPlanning(false);
      if (!e.payload.ok || !e.payload.result) {
        setError(e.payload.message);
        return;
      }
      const r = e.payload.result;
      if (r.note) onNotice(r.note);
      else onNotice(`${t("autoEditDone")} (${t(`planner_${r.planner}` as TKey)}${r.cost ? ` · $${r.cost.toFixed(4)}` : ""})`);
      onOpen(r.project);
    });
    return () => {
      un.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Ask for the cost again whenever what is sent changes (Claude only).
  useEffect(() => {
    setEstimate(null);
    if (ai?.planner !== "claude" || !shots) return;
    api5.estimatePlan(brief(), shots).then(setEstimate).catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ai?.planner, shots, idea, shape, seconds, pace]);

  const pick = async () => {
    const picked = await openDialog({ multiple: true, filters: [{ name: "Media", extensions: [...VIDEO_EXT, ...IMAGE_EXT, ...AUDIO_EXT] }] });
    if (!picked) return;
    const list = Array.isArray(picked) ? picked : [picked];
    setFiles((f) => [...f, ...list.filter((p) => !f.includes(p))]);
    setShots(null);
  };
  const run = async () => {
    setError(null);
    if (!shots) {
      setAnalyzing({ done: 0, total: files.length });
      api5.analyzeShots(files).catch((e) => setError(String(e)));
      return;
    }
    setPlanning(true);
    api5.makeEdit(brief(), shots, idea.slice(0, 40) || t("autoEdit")).catch((e) => {
      setPlanning(false);
      setError(String(e));
    });
  };
  const overCap = !!estimate && estimate.spent + estimate.dollars > estimate.cap;
  return (
    <div className="panel-body auto-edit">
      <h4>{t("autoEdit")}</h4>
      <p className="hint small">{t("autoEditHint")}</p>
      <label>
        {t("yourIdea")}
        <textarea dir="auto" rows={4} placeholder={t("ideaPlaceholder")} value={idea} onChange={(e) => setIdea(e.target.value)} />
      </label>
      <div className="row">
        <button onClick={pick}>{t("addShots")}</button>
        {files.length > 0 && (
          <button
            onClick={() => {
              setFiles([]);
              setShots(null);
            }}
          >
            {t("clear")}
          </button>
        )}
      </div>
      {files.length > 0 && (
        <ul className="file-list">
          {files.map((f) => (
            <li key={f} dir="auto">
              {fileName(f)}
              <button aria-label={t("remove")} onClick={() => {
                setFiles((x) => x.filter((y) => y !== f));
                setShots(null);
              }}>
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
      <span className="label">{t("shape")}</span>
      <div className="chips">
        {SHAPES_OF.map((s, i) => (
          <button key={s.key} className={shape === i ? "on" : ""} onClick={() => setShape(i)}>
            {t(s.key as TKey)}
          </button>
        ))}
      </div>
      <span className="label">{t("pace")}</span>
      <div className="chips">
        {(["", "fast", "calm"] as const).map((p) => (
          <button key={p} className={pace === p ? "on" : ""} onClick={() => setPace(p)}>
            {t(p === "" ? "paceAuto" : p === "fast" ? "paceFast" : "paceCalm")}
          </button>
        ))}
      </div>
      <Slider label={t("length")} min={0} max={180} step={5} value={seconds} onChange={setSeconds} format={(v) => (v ? `${v} ${t("seconds")}` : t("autoLength"))} />
      <button className="link" onClick={() => setSettings((s) => !s)}>
        {t("aiSettings")}: {ai ? t(`planner_${ai.planner}` as TKey) : "…"} {settings ? "▲" : "▼"}
      </button>
      <div hidden={!settings}>
        <AiSettings onChange={setAi} />
      </div>
      {estimate && (
        <p className={`hint small ${overCap ? "error" : ""}`}>
          {t("estimate")}: <span dir="ltr">~${estimate.dollars.toFixed(4)}</span> · {t("spentThisMonth")} <span dir="ltr">${estimate.spent.toFixed(3)} / ${estimate.cap.toFixed(2)}</span>
          {overCap && <> · {t("overCap")}</>}
        </p>
      )}
      <Progress busy={analyzing} label={t("lookingAtShots")} />
      {planning && <p className="hint small">{t("planning")}…</p>}
      {!analyzing && !planning && (
        <button className="primary wide" disabled={!files.length || !idea.trim()} onClick={run}>
          {shots ? t("makeEdit") : t("lookAtShots")}
        </button>
      )}
      {shots && <p className="hint small">{t("shotsReady").replace("{n}", String(shots.length))}</p>}
      {error && <p className="hint small error">{error}</p>}
    </div>
  );
}

// ---------- cut to the beat ----------

export function BeatSection({ project, onCommit }: { project: Project; onCommit: (p: Project) => void }) {
  const t = useT();
  const fps = fpsValue(project.fps);
  const music = allClips(project).filter((c) => c.media === "audio");
  const videoTracks = project.tracks.filter((x) => x.kind === "video" && x.clips.length > 1);
  const [musicId, setMusicId] = useState(music[0]?.id ?? "");
  const [trackId, setTrackId] = useState(videoTracks[0]?.id ?? "");
  const [every, setEvery] = useState(2);
  const [beats, setBeats] = useState<Beats | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const m = music.find((c) => c.id === musicId) ?? music[0];
  const tr = videoTracks.find((x) => x.id === trackId) ?? videoTracks[0];
  if (!m) return <p className="hint small">{t("beatNeedsMusic")}</p>;
  if (!tr) return <p className="hint small">{t("beatNeedsClips")}</p>;
  const frames = (b: Beats, c: Clip) => b.times.map((s) => c.start + Math.round(((s - c.source_in) * fps) / (c.speed ?? 1))).filter((f) => f >= c.start && f <= c.start + c.duration);
  const find = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const b = await api5.detectBeats(m.source, m.source_in, (m.duration / fps) * (m.speed ?? 1));
      setBeats(b);
      setMsg(`${t("tempo")}: ${Math.round(b.tempo)} BPM · ${b.times.length} ${t("beats")}`);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };
  const apply = async () => {
    if (!beats) return;
    const lengths = new Map<string, number>();
    for (const c of tr.clips) if (c.media === "video") lengths.set(c.id, (await mediaInfo(c.source)).duration);
    onCommit(cutToBeats(project, tr.id, frames(beats, m), every, (c) => lengths.get(c.id) ?? Infinity));
    setMsg(t("beatDone"));
  };
  return (
    <>
      <label>
        {t("music")}
        <select value={m.id} onChange={(e) => {
          setMusicId(e.target.value);
          setBeats(null);
        }}>
          {music.map((c) => (
            <option key={c.id} value={c.id}>
              {fileName(c.source)}
            </option>
          ))}
        </select>
      </label>
      {videoTracks.length > 1 && (
        <label>
          {t("clipsTrack")}
          <select value={tr.id} onChange={(e) => setTrackId(e.target.value)}>
            {videoTracks.map((x, i) => (
              <option key={x.id} value={x.id}>
                V{i + 1}
              </option>
            ))}
          </select>
        </label>
      )}
      <span className="label">{t("cutEvery")}</span>
      <div className="chips">
        {[1, 2, 4, 8].map((n) => (
          <button key={n} className={every === n ? "on" : ""} onClick={() => setEvery(n)}>
            {n}
          </button>
        ))}
      </div>
      <div className="row">
        <button disabled={busy} onClick={find}>
          {busy ? `${t("listening")}…` : t("findBeats")}
        </button>
        <button className="primary" disabled={!beats} onClick={apply}>
          {t("cutToBeat")}
        </button>
      </div>
      {msg && <p className="hint small">{msg}</p>}
    </>
  );
}

// ---------- follow the subject ----------

export function ReframeSection({ project, onOpen }: { project: Project; onOpen: (p: Project) => void }) {
  const t = useT();
  const [models, setModels] = useState<{ id: string; mb: number; downloaded: boolean }[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const [target, setTarget] = useState(0);
  const [busy, setBusy] = useJob<Project>("reframe", (ok, m, result) => {
    if (ok && result) {
      onOpen({ ...result, name: `${project.name} ${t(SHAPES_OF[target].key as TKey)}` });
      setMsg(t("reframeDone"));
    } else setMsg(m);
  });
  useEffect(() => {
    api.matteModels().then(setModels).catch(() => {});
  }, []);
  const model = models.find((m) => m.downloaded);
  const options = SHAPES_OF.filter((s) => s.w * project.height !== s.h * project.width);
  return (
    <>
      <p className="hint small">{t("reframeHint")}</p>
      <div className="chips">
        {options.map((s) => {
          const i = SHAPES_OF.indexOf(s);
          return (
            <button key={s.key} className={target === i ? "on" : ""} onClick={() => setTarget(i)}>
              {t(s.key as TKey)}
            </button>
          );
        })}
      </div>
      {!model && <p className="hint small">{t("reframeNeedsModel")}</p>}
      {!busy && (
        <button
          className="primary"
          disabled={!model || projectDuration(project) === 0 || !options.includes(SHAPES_OF[target] as (typeof options)[number])}
          onClick={() => {
            setMsg(null);
            setBusy({ done: 0, total: 1 });
            api5.autoReframe(SHAPES_OF[target].w, SHAPES_OF[target].h, model!.id).catch((e) => {
              setBusy(null);
              setMsg(String(e));
            });
          }}
        >
          {t("followSubject")}
        </button>
      )}
      <Progress busy={busy} label={t("followingSubject")} />
      {msg && <p className="hint small">{msg}</p>}
    </>
  );
}
