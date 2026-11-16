import { useEffect, useMemo, useRef, useState } from "react";
import { mediaInfo, thumbnailOf, waveformOf } from "./api";
import { addTrack, place, removeTrack, snap } from "./edits";
import { speedOf } from "./motion";
import { removeMarker, roll, slip } from "./edits5";
import { useT } from "./i18n";
import type { Clip, Project, Shape, TextLayer, Track } from "./types";
import { fileName, formatTime, fpsValue, projectDuration } from "./types";

interface Props {
  project: Project;
  frame: number;
  selected: string | null;
  onSeek: (f: number) => void;
  onSelect: (id: string | null) => void;
  onCommit: (p: Project) => void;
  /** Clips picked together with Ctrl or Shift (besides `selected`). */
  picked?: string[];
  onPick?: (ids: string[]) => void;
}

type Mode = "move" | "trimStart" | "trimEnd" | "slip" | "rollStart" | "rollEnd";
type Drag =
  | { kind: "clip"; mode: Mode; id: string; x0: number; orig: Clip; trackId: string }
  | { kind: "text"; mode: Mode; id: string; x0: number; orig: TextLayer }
  | { kind: "shape"; mode: Mode; id: string; x0: number; orig: Shape };

const HEADER = 128;
const SHAPE_ICON: Record<Shape["kind"], string> = { rect: "▬", ellipse: "●", line: "━", arrow: "➜", star: "★" };
const EDGE = 7;

/** Texts that overlap in time go on separate rows so none hides another. */
function textLanes<T extends { start: number; duration: number }>(texts: T[]): T[][] {
  const lanes: T[][] = [];
  for (const t of [...texts].sort((a, b) => a.start - b.start)) {
    const free = lanes.find((l) => l.every((o) => t.start >= o.start + o.duration || t.start + t.duration <= o.start));
    if (free) free.push(t);
    else lanes.push([t]);
  }
  return lanes.length ? lanes : [[]];
}

export function Timeline({ project, frame, selected, onSeek, onSelect, onCommit, picked = [], onPick }: Props) {
  const t = useT();
  const scroller = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const [zoom, setZoom] = useState(1);
  const [magnet, setMagnet] = useState(true);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [draft, setDraft] = useState<Project | null>(null);
  const [durations, setDurations] = useState<Record<string, number>>({});
  const shown = draft ?? project;
  const fps = fpsValue(project.fps);

  const total = Math.max(projectDuration(project), Math.round(fps * 10));
  // At zoom 1 the whole project (plus a little room) fits the visible width.
  const ppf = ((width - HEADER - 24) / (total * 1.1)) * zoom;
  const x = (f: number) => f * ppf;

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Source lengths, so a trim can't go past the end of a file.
  useEffect(() => {
    for (const c of project.tracks.flatMap((tr) => tr.clips)) {
      if (c.media === "image" || c.media === "compound" || durations[c.source] !== undefined) continue;
      mediaInfo(c.source)
        .then((i) => setDurations((d) => ({ ...d, [c.source]: i.duration })))
        .catch(() => {});
    }
  }, [project, durations]);

  const lanes = useMemo(() => textLanes(shown.texts), [shown.texts]);
  const shapeLanes = useMemo(() => (shown.shapes?.length ? textLanes(shown.shapes) : []), [shown.shapes]);
  // How long a clip's source is, in seconds (a group's own length for group clips).
  const sourceSeconds = (c: Clip) => {
    if (c.media === "image" || c.freeze) return Infinity;
    if (c.media === "compound") {
      const g = project.compounds?.find((x) => x.id === c.source);
      return g ? Math.max(0, ...g.tracks.flatMap((tr) => tr.clips.map((y) => y.start + y.duration))) / fps : Infinity;
    }
    return durations[c.source] ?? Infinity;
  };
  const videoTracks = [...shown.tracks.filter((tr) => tr.kind === "video")].reverse();
  const audioTracks = shown.tracks.filter((tr) => tr.kind === "audio");

  const frameAt = (clientX: number) => {
    const r = scroller.current!.getBoundingClientRect();
    return Math.max(0, Math.round((clientX - r.left - HEADER + scroller.current!.scrollLeft) / ppf));
  };

  const tol = Math.round(8 / ppf);
  const snapped = (f: number, ignore: string) => (magnet ? snap(project, f, tol, frame, ignore) : f);

  const startDrag = (e: React.PointerEvent, d: Drag) => {
    e.stopPropagation();
    // Ctrl or Shift: add to (or take out of) the picked clips instead of dragging.
    if (d.kind === "clip" && (e.ctrlKey || e.metaKey || e.shiftKey) && onPick) {
      const all = new Set([...picked, ...(selected ? [selected] : [])]);
      if (all.has(d.id)) all.delete(d.id);
      else all.add(d.id);
      onPick([...all]);
      return;
    }
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSelect(d.id);
    onPick?.([]);
    setDrag(d);
  };

  // Alt (Option) changes the tools: middle = slip, edge = roll.
  const modeFor = (e: React.PointerEvent): Mode => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const edge = e.clientX - r.left < EDGE ? "trimStart" : r.right - e.clientX < EDGE ? "trimEnd" : "move";
    if (e.altKey) return edge === "move" ? "slip" : edge === "trimStart" ? "rollStart" : "rollEnd";
    return edge;
  };
  const plainMode = (e: React.PointerEvent): Mode => {
    const m = modeFor(e);
    return m === "slip" ? "move" : m === "rollStart" ? "trimStart" : m === "rollEnd" ? "trimEnd" : m;
  };

  const onMove = (e: React.PointerEvent) => {
    if (!drag) return;
    const df = Math.round((e.clientX - drag.x0) / ppf);
    if (drag.kind === "text" || drag.kind === "shape") {
      const o = drag.orig as TextLayer | Shape;
      let next = { ...o };
      if (drag.mode === "move") {
        let s = Math.max(0, o.start + df);
        const ss = snapped(s, o.id);
        const se = snapped(s + o.duration, o.id) - o.duration;
        s = ss !== s ? ss : se;
        next.start = Math.max(0, s);
      } else if (drag.mode === "trimStart") {
        const s = Math.min(o.start + o.duration - 1, Math.max(0, snapped(o.start + df, o.id)));
        next = { ...o, start: s, duration: o.duration + (o.start - s) };
      } else {
        const end = Math.max(o.start + 1, snapped(o.start + o.duration + df, o.id));
        next.duration = end - o.start;
      }
      if (drag.kind === "text") setDraft({ ...project, texts: project.texts.map((x) => (x.id === o.id ? (next as TextLayer) : x)) });
      else setDraft({ ...project, shapes: (project.shapes ?? []).map((x) => (x.id === o.id ? (next as Shape) : x)) });
      return;
    }
    const o = drag.orig;
    const srcLen = sourceSeconds(o);
    if (drag.mode === "slip") {
      // Dragging right shows an earlier part, like pulling the film along.
      setDraft(place(project, drag.trackId, slip(o, -df, fps, srcLen)));
      return;
    }
    if (drag.mode === "rollStart" || drag.mode === "rollEnd") {
      const tr = project.tracks.find((x) => x.id === drag.trackId)!;
      const left = tr.clips.find((c) => c.start + c.duration === o.start);
      const right = tr.clips.find((c) => c.start === o.start + o.duration);
      const pair = drag.mode === "rollStart" ? (left ? [left, o] : null) : right ? [o, right] : null;
      if (pair) setDraft(roll(project, pair[0].id, pair[1].id, df, sourceSeconds));
      return;
    }
    let next = { ...o };
    let trackId = drag.trackId;
    if (drag.mode === "move") {
      let s = Math.max(0, o.start + df);
      const ss = snapped(s, o.id);
      const se = snapped(s + o.duration, o.id) - o.duration;
      s = ss !== s ? ss : se;
      next.start = Math.max(0, s);
      // Moving up or down onto another track of the same kind.
      const el = document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-track]") as HTMLElement | null;
      const target = el && project.tracks.find((tr) => tr.id === el.dataset.track);
      const isAudioClip = o.media === "audio";
      if (target && !target.locked && (target.kind === "audio") === isAudioClip) trackId = target.id;
    } else if (drag.mode === "trimStart") {
      const minStart = o.media === "image" ? 0 : Math.max(0, o.start - Math.floor((o.source_in * fps) / speedOf(o)));
      let s = snapped(o.start + df, o.id);
      s = Math.min(o.start + o.duration - 1, Math.max(minStart, s));
      const d = s - o.start;
      next = { ...o, start: s, duration: o.duration - d, source_in: o.media === "image" ? 0 : o.source_in + (d / fps) * speedOf(o) };
    } else {
      const maxEnd = o.start + Math.floor(((srcLen - o.source_in) * fps) / speedOf(o));
      let end = snapped(o.start + o.duration + df, o.id);
      end = Math.max(o.start + 1, Math.min(maxEnd, end));
      next.duration = end - o.start;
    }
    setDraft(place(project, trackId, next));
  };

  const endDrag = () => {
    if (draft) onCommit(draft);
    setDraft(null);
    setDrag(null);
  };

  const setTrack = (tr: Track, patch: Partial<Track>) =>
    onCommit({ ...project, tracks: project.tracks.map((x) => (x.id === tr.id ? { ...x, ...patch } : x)) });

  const rulerStep = (() => {
    const secPx = ppf * fps;
    for (const s of [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300]) if (s * secPx >= 70) return s;
    return 600;
  })();
  const contentFrames = Math.max(total * 1.1, (width - HEADER) / ppf);
  const ticks: number[] = [];
  for (let s = 0; s * fps <= contentFrames; s += rulerStep) ticks.push(s);

  const header = (tr: Track | null, label: string) => (
    <div className="track-head">
      <span className="track-name">{label}</span>
      {tr && (
        <span className="track-buttons">
          <button className={tr.muted ? "on" : ""} title={t("mute")} onClick={() => setTrack(tr, { muted: !tr.muted })}>
            M
          </button>
          {tr.kind === "audio" && (
            <button className={tr.duck ? "on" : ""} title={t("duckTitle")} onClick={() => setTrack(tr, { duck: !tr.duck })}>
              ↓♪
            </button>
          )}
          {tr.kind === "video" && (
            <button className={tr.hidden ? "on" : ""} title={t("hide")} onClick={() => setTrack(tr, { hidden: !tr.hidden })}>
              H
            </button>
          )}
          <button className={tr.locked ? "on" : ""} title={t("lock")} onClick={() => setTrack(tr, { locked: !tr.locked })}>
            L
          </button>
          <button title={t("removeTrack")} onClick={() => onCommit(removeTrack(project, tr.id))}>
            ×
          </button>
        </span>
      )}
    </div>
  );

  const clipEl = (c: Clip, tr: Track) => (
    <ClipView
      key={c.id}
      clip={c}
      left={x(c.start)}
      width={Math.max(2, x(c.duration))}
      selected={selected === c.id || picked.includes(c.id)}
      fps={fps}
      name={c.media === "compound" ? (project.compounds?.find((g) => g.id === c.source)?.name ?? "") : undefined}
      sourceDuration={durations[c.source]}
      onPointerDown={(e) => {
        if (tr.locked) return;
        startDrag(e, { kind: "clip", mode: modeFor(e), id: c.id, x0: e.clientX, orig: c, trackId: tr.id });
      }}
    />
  );

  return (
    <div className="timeline" dir="ltr">
      <div className="timeline-bar">
        <span className="time">{formatTime(frame, project.fps)}</span>
        <span className="muted">/ {formatTime(projectDuration(project), project.fps)}</span>
        <div className="spacer" />
        <button onClick={() => onCommit(addTrack(project, "video"))}>{t("videoTrack")}</button>
        <button onClick={() => onCommit(addTrack(project, "audio"))}>{t("audioTrack")}</button>
        <button className={magnet ? "on" : ""} onClick={() => setMagnet((m) => !m)}>
          {t("magnet")}
        </button>
        <label className="zoom">
          {t("zoom")}
          <input type="range" min={1} max={40} step={0.1} value={zoom} onChange={(e) => setZoom(+e.target.value)} />
        </label>
      </div>
      <div
        className="timeline-scroll"
        ref={scroller}
        onWheel={(e) => {
          if (e.ctrlKey || e.metaKey) {
            e.preventDefault();
            setZoom((z) => Math.min(40, Math.max(1, z * (e.deltaY < 0 ? 1.15 : 1 / 1.15))));
          }
        }}
        onPointerMove={onMove}
        onPointerUp={endDrag}
      >
        <div className="timeline-content" style={{ width: HEADER + x(contentFrames) }}>
          <div
            className="ruler"
            onPointerDown={(e) => {
              (e.target as HTMLElement).setPointerCapture(e.pointerId);
              onSeek(frameAt(e.clientX));
            }}
            onPointerMove={(e) => e.buttons === 1 && !drag && onSeek(frameAt(e.clientX))}
          >
            <div className="track-head" />
            {(project.markers ?? []).map((m) => (
              <span
                key={m.frame}
                className="marker"
                style={{ left: HEADER + x(m.frame), background: m.color }}
                title={`${m.label || t("marker")} · ${formatTime(m.frame, project.fps)} · ${t("markerRemoveHint")}`}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  onSeek(m.frame);
                }}
                onDoubleClick={(e) => {
                  e.stopPropagation();
                  onCommit(removeMarker(project, m.frame));
                }}
              />
            ))}
            {ticks.map((s) => (
              <span key={s} className="tick" style={{ left: HEADER + x(s * fps) }}>
                {formatTime(Math.round(s * fps), project.fps).replace(/\.00$/, "")}
              </span>
            ))}
          </div>

          {lanes.map((lane, i) => (
            <div className="row text-row" key={`l${i}`}>
              {header(null, i === 0 ? t("text") : "")}
              <div className="lane" onPointerDown={() => onSelect(null)}>
                {lane.map((tx) => (
                  <div
                    key={tx.id}
                    className={`item text ${selected === tx.id ? "sel" : ""}`}
                    style={{ left: x(tx.start), width: Math.max(2, x(tx.duration)) }}
                    onPointerDown={(e) => startDrag(e, { kind: "text", mode: plainMode(e), id: tx.id, x0: e.clientX, orig: tx })}
                    dir="auto"
                  >
                    {tx.text}
                  </div>
                ))}
              </div>
            </div>
          ))}

          {shapeLanes.map((lane, i) => (
            <div className="row shape-row" key={`s${i}`}>
              {header(null, i === 0 ? t("shapes") : "")}
              <div className="lane" onPointerDown={() => onSelect(null)}>
                {lane.map((sh) => (
                  <div
                    key={sh.id}
                    className={`item shape ${selected === sh.id ? "sel" : ""}`}
                    style={{ left: x(sh.start), width: Math.max(2, x(sh.duration)) }}
                    onPointerDown={(e) => startDrag(e, { kind: "shape", mode: plainMode(e), id: sh.id, x0: e.clientX, orig: sh })}
                  >
                    {SHAPE_ICON[sh.kind]}
                  </div>
                ))}
              </div>
            </div>
          ))}

          {videoTracks.map((tr, i) => (
            <div className={`row video-row ${tr.hidden ? "dim" : ""}`} key={tr.id}>
              {header(tr, `V${videoTracks.length - i}`)}
              <div className={`lane ${tr.locked ? "locked" : ""}`} data-track={tr.id} onPointerDown={() => onSelect(null)}>
                {tr.clips.map((c) => clipEl(c, tr))}
              </div>
            </div>
          ))}
          {audioTracks.map((tr, i) => (
            <div className={`row audio-row ${tr.muted ? "dim" : ""}`} key={tr.id}>
              {header(tr, `A${i + 1}`)}
              <div className={`lane ${tr.locked ? "locked" : ""}`} data-track={tr.id} onPointerDown={() => onSelect(null)}>
                {tr.clips.map((c) => clipEl(c, tr))}
              </div>
            </div>
          ))}

          {projectDuration(project) === 0 && <div className="timeline-empty">{t("emptyTimeline")}</div>}
          <div className="playhead" style={{ left: HEADER + x(frame) }} />
        </div>
      </div>
    </div>
  );
}

function ClipView(props: {
  clip: Clip;
  left: number;
  width: number;
  selected: boolean;
  fps: number;
  sourceDuration?: number;
  /** A group's name, for group clips. */
  name?: string;
  onPointerDown: (e: React.PointerEvent) => void;
}) {
  const { clip, left, width, selected } = props;
  const [thumb, setThumb] = useState<string | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (clip.media === "audio" || clip.media === "compound") return;
    thumbnailOf(clip.source).then(setThumb).catch(() => {});
  }, [clip.source, clip.media]);

  // Waveform of the part of the file this clip plays.
  useEffect(() => {
    if (clip.media === "image" || clip.media === "compound" || clip.freeze || clip.reverse || !props.sourceDuration) return;
    let alive = true;
    waveformOf(clip.source).then((peaks) => {
      const c = canvas.current;
      if (!alive || !c || !peaks.length) return;
      const w = Math.max(1, Math.floor(width));
      const h = c.clientHeight || 24;
      c.width = w;
      c.height = h;
      const ctx = c.getContext("2d")!;
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = "rgba(255,255,255,0.55)";
      const from = clip.source_in / props.sourceDuration!;
      const span = ((clip.duration / props.fps) * speedOf(clip)) / props.sourceDuration!;
      for (let px = 0; px < w; px++) {
        const i = Math.floor((from + (px / w) * span) * peaks.length);
        const v = (peaks[Math.min(peaks.length - 1, Math.max(0, i))] ?? 0) * h;
        ctx.fillRect(px, (h - v) / 2, 1, Math.max(1, v));
      }
    });
    return () => {
      alive = false;
    };
  }, [clip.source, clip.source_in, clip.duration, clip.media, clip.speed, width, props.fps, props.sourceDuration]);

  return (
    <div
      className={`item clip ${clip.media} ${selected ? "sel" : ""}`}
      style={{ left, width, backgroundImage: thumb ? `url(${thumb})` : undefined }}
      onPointerDown={props.onPointerDown}
      title={props.name ?? fileName(clip.source)}
    >
      <span className="clip-name">{clip.media === "compound" ? `▣ ${props.name ?? ""}` : fileName(clip.source)}</span>
      {clip.media !== "image" && clip.media !== "compound" && <canvas ref={canvas} className="wave" />}
      {clip.reverse && <span className="badge5">◀◀</span>}
      {clip.freeze && <span className="badge5">❚❚</span>}
      {(clip.stabilize ?? 0) > 0 && <span className="badge5">≋</span>}
      {clip.transition_in && <span className="trans" style={{ width: (clip.transition_in.frames / clip.duration) * 100 + "%" }} />}
      {speedOf(clip) !== 1 && <span className="speed">{speedOf(clip)}×</span>}
      {!!clip.keyframes?.length && <span className="moving">◆</span>}
      {clip.fade_in > 0 && <span className="fade in" style={{ width: (clip.fade_in / clip.duration) * 100 + "%" }} />}
      {clip.fade_out > 0 && <span className="fade out" style={{ width: (clip.fade_out / clip.duration) * 100 + "%" }} />}
    </div>
  );
}
