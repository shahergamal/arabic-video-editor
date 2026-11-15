// Timeline edits. Each returns a new project and never changes the old one,
// which is what makes undo safe.
import type { Clip, Project } from "./types";
import { newId } from "./types";

/** Splits the clip under `frame` into two clips at that frame. */
export function splitAt(p: Project, frame: number): Project {
  const fps = p.fps.num / p.fps.den;
  const clips: Clip[] = [];
  for (const c of p.clips) {
    if (frame > c.start && frame < c.start + c.duration) {
      const first = frame - c.start;
      clips.push({ ...c, duration: first });
      clips.push({ ...c, id: newId(), start: frame, duration: c.duration - first, source_in: c.source_in + first / fps });
    } else clips.push(c);
  }
  return { ...p, clips };
}

/** Removes a clip and pulls everything after it left to close the gap. */
export function rippleDelete(p: Project, clipId: string): Project {
  const gone = p.clips.find((c) => c.id === clipId);
  if (!gone) return p;
  const end = gone.start + gone.duration;
  const shift = <T extends { start: number }>(x: T): T => (x.start >= end ? { ...x, start: x.start - gone.duration } : x);
  return {
    ...p,
    clips: p.clips.filter((c) => c.id !== clipId).map(shift),
    texts: p.texts.map(shift),
  };
}

/** Puts a new clip at the end of the timeline. */
export function appendClip(p: Project, source: string, seconds: number): Project {
  const fps = p.fps.num / p.fps.den;
  const start = p.clips.reduce((m, c) => Math.max(m, c.start + c.duration), 0);
  const duration = Math.max(1, Math.floor(seconds * fps));
  return { ...p, clips: [...p.clips, { id: newId(), source, start, duration, source_in: 0, volume: 1 }] };
}
