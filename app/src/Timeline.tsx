import { useEffect, useMemo, useRef, useState } from "react";
import { mediaInfo, thumbnailOf, waveformOf } from "./api";
import { addTrack, place, removeTrack, snap } from "./edits";
import { speedOf } from "./motion";
import { useT } from "./i18n";
import type { Clip, Project, TextLayer, Track } from "./types";
import { fileName, formatTime, fpsValue, projectDuration } from "./types";

interface Props {
  project: Project;
  frame: number;
  selected: string | null;
  onSeek: (f: number) => void;
  onSelect: (id: string | null) => void;
  onCommit: (p: Project) => void;
}

type Drag =
  | { kind: "clip"; mode: "move" | "trimStart" | "trimEnd"; id: string; x0: number; orig: Clip; trackId: string }
  | { kind: "text"; mode: "move" | "trimStart" | "trimEnd"; id: string; x0: number; orig: TextLayer };

const HEADER = 128;
const EDGE = 7;

/** Texts that overlap in time go on separate rows so none hides another. */
function textLanes(texts: TextLayer[]): TextLayer[][] {
  const lanes: TextLayer[][] = [];
  for (const t of [...texts].sort((a, b) => a.start - b.start)) {
    const free = lanes.find((l) => l.every((o) => t.start >= o.start + o.duration || t.start + t.duration <= o.start));
    if (free) free.push(t);
    else lanes.push([t]);
  }
  return lanes.length ? lanes : [[]];
}

export function Timeline({ project, frame, selected, onSeek, onSelect, onCommit }: Props) {
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
      if (c.media === "image" || durations[c.source] !== undefined) continue;
      mediaInfo(c.source)
        .then((i) => setDurations((d) => ({ ...d, [c.source]: i.duration })))
        .catch(() => {});
    }
  }, [project, durations]);

  const lanes = useMemo(() => textLanes(shown.texts), [shown.texts]);
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
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSelect(d.id);
    setDrag(d);
  };

  const modeFor = (e: React.PointerEvent): "move" | "trimStart" | "trimEnd" => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    if (e.clientX - r.left < EDGE) return "trimStart";
    if (r.right - e.clientX < EDGE) return "trimEnd";
    return "move";
  };

  const onMove = (e: React.PointerEvent) => {
    if (!drag) return;
    const df = Math.round((e.clientX - drag.x0) / ppf);
    if (drag.kind === "text") {
      const o = drag.orig;
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
      setDraft({ ...project, texts: project.texts.map((x) => (x.id === o.id ? next : x)) });
      return;
    }
    const o = drag.orig;
    const srcLen = o.media === "image" ? Infinity : (durations[o.source] ?? Infinity);
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
      selected={selected === c.id}
      fps={fps}
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
                    onPointerDown={(e) => startDrag(e, { kind: "text", mode: modeFor(e), id: tx.id, x0: e.clientX, orig: tx })}
                    dir="auto"
                  >
                    {tx.text}
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
  onPointerDown: (e: React.PointerEvent) => void;
}) {
  const { clip, left, width, selected } = props;
  const [thumb, setThumb] = useState<string | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (clip.media === "audio") return;
    thumbnailOf(clip.source).then(setThumb).catch(() => {});
  }, [clip.source, clip.media]);

  // Waveform of the part of the file this clip plays.
  useEffect(() => {
    if (clip.media === "image" || !props.sourceDuration) return;
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
      title={fileName(clip.source)}
    >
      <span className="clip-name">{fileName(clip.source)}</span>
      {clip.media !== "image" && <canvas ref={canvas} className="wave" />}
      {clip.transition_in && <span className="trans" style={{ width: (clip.transition_in.frames / clip.duration) * 100 + "%" }} />}
      {speedOf(clip) !== 1 && <span className="speed">{speedOf(clip)}×</span>}
      {!!clip.keyframes?.length && <span className="moving">◆</span>}
      {clip.fade_in > 0 && <span className="fade in" style={{ width: (clip.fade_in / clip.duration) * 100 + "%" }} />}
      {clip.fade_out > 0 && <span className="fade out" style={{ width: (clip.fade_out / clip.duration) * 100 + "%" }} />}
    </div>
  );
}
