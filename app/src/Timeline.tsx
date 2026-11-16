import { useRef } from "react";
import type { Project, TextLayer } from "./types";
import { formatTime, projectDuration } from "./types";

interface Props {
  project: Project;
  frame: number;
  selected: string | null;
  onSeek: (f: number) => void;
  onSelect: (id: string | null) => void;
}

export function Timeline({ project, frame, selected, onSeek, onSelect }: Props) {
  const area = useRef<HTMLDivElement>(null);
  const total = Math.max(projectDuration(project), Math.round(project.fps.num / project.fps.den) * 10);
  const pct = (f: number) => `${(f / total) * 100}%`;
  const name = (path: string) => path.split(/[\\/]/).pop();

  // Texts that overlap in time go on separate rows so none hides another.
  const lanes: TextLayer[][] = [];
  for (const t of [...project.texts].sort((a, b) => a.start - b.start)) {
    const free = lanes.find((l) => l.every((o) => t.start >= o.start + o.duration || t.start + t.duration <= o.start));
    if (free) free.push(t);
    else lanes.push([t]);
  }
  if (lanes.length === 0) lanes.push([]);

  // The timeline runs left to right like every video editor, even in Arabic,
  // so time reads the same as the picture plays.
  const seekFromMouse = (clientX: number) => {
    const r = area.current!.getBoundingClientRect();
    const f = Math.round(((clientX - r.left) / r.width) * total);
    onSeek(Math.max(0, Math.min(total - 1, f)));
  };

  return (
    <div className="timeline" dir="ltr">
      <div className="ruler">
        <span>{formatTime(frame, project.fps)}</span>
        <span className="muted">/ {formatTime(projectDuration(project), project.fps)}</span>
      </div>
      <div
        className="tracks"
        ref={area}
        style={{ gridTemplateRows: `repeat(${lanes.length}, 26px) 44px` }}
        onPointerDown={(e) => {
          (e.target as HTMLElement).setPointerCapture(e.pointerId);
          seekFromMouse(e.clientX);
        }}
        onPointerMove={(e) => e.buttons === 1 && seekFromMouse(e.clientX)}
      >
        {lanes.map((lane, i) => (
          <div className="track text-track" key={i} aria-label="النصوص">
            {lane.map((t) => (
              <div
                key={t.id}
                className={`item text ${selected === t.id ? "sel" : ""}`}
                style={{ left: pct(t.start), width: pct(t.duration) }}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  onSelect(t.id);
                }}
                dir="auto"
              >
                {t.text}
              </div>
            ))}
          </div>
        ))}
        <div className="track" aria-label="الفيديو">
          {project.clips.map((c) => (
            <div
              key={c.id}
              className={`item clip ${selected === c.id ? "sel" : ""}`}
              style={{ left: pct(c.start), width: pct(c.duration) }}
              onPointerDown={(e) => {
                e.stopPropagation();
                onSelect(c.id);
              }}
            >
              {name(c.source)}
            </div>
          ))}
        </div>
        <div className="playhead" style={{ left: pct(frame) }} />
      </div>
    </div>
  );
}
