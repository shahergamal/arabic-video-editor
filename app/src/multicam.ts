// Several cameras of one shoot, lined up by sound. The cut is one video
// track ("mc") that switches between cameras; the sound comes from the first
// camera on its own audio clip, so it never jumps when the picture does.
import { splitAt } from "./edits";
import type { Angle, Clip, Project } from "./types";
import { IDENTITY, fileName, fpsValue, newId } from "./types";

export const MC_TRACK = "mc";

/** Adds a multi-camera cut after whatever is on the timeline. */
export function buildMulticam(p: Project, angles: Angle[]): Project {
  if (angles.length < 2) return p;
  const fps = fpsValue(p.fps);
  // The part every camera can show: from the latest start to the earliest end.
  const from = Math.max(0, ...angles.map((a) => a.offset));
  const to = Math.min(...angles.map((a) => a.offset + a.duration));
  const first = angles[0];
  const span = to > from ? [from, to] : [0, first.duration];
  const duration = Math.max(1, Math.floor((span[1] - span[0]) * fps));
  const at = p.tracks.reduce((m, t) => Math.max(m, ...t.clips.map((c) => c.start + c.duration)), 0);
  const base = { start: at, duration, volume: 1, transform: IDENTITY, fade_in: 0, fade_out: 0 };
  const picture: Clip = { ...base, id: newId(), source: first.source, source_in: span[0] - first.offset, media: "video" };
  const sound: Clip = { ...base, id: newId(), source: first.source, source_in: span[0] - first.offset, media: "audio" };
  const named = angles.map((a, i) => ({ ...a, name: a.name || `${i + 1} · ${fileName(a.source)}` }));
  const tracks = p.tracks.filter((t) => t.id !== MC_TRACK);
  const mc = { id: MC_TRACK, kind: "video" as const, muted: true, hidden: false, locked: false, clips: [...(p.tracks.find((t) => t.id === MC_TRACK)?.clips ?? []), picture] };
  const audio = tracks.findIndex((t) => t.kind === "audio");
  const withSound = tracks.map((t, i) => (i === audio ? { ...t, clips: [...t.clips, sound] } : t));
  // The camera track sits just above the other picture tracks.
  const lastVideo = withSound.map((t) => t.kind).lastIndexOf("video");
  withSound.splice(lastVideo + 1, 0, mc);
  return { ...p, angles: named, tracks: withSound };
}

/** Which camera a clip shows, by its file. */
export const angleOf = (p: Project, c: Clip) => (p.angles ?? []).findIndex((a) => a.source === c.source);

/**
 * From `frame` on, shows camera `k` instead, at the same moment. Returns the
 * project unchanged when that camera was not recording then.
 */
export function switchAngle(p: Project, frame: number, k: number): Project {
  const angles = p.angles ?? [];
  const target = angles[k];
  const track = p.tracks.find((t) => t.id === MC_TRACK);
  const clip = track?.clips.find((c) => frame >= c.start && frame < c.start + c.duration);
  if (!target || !track || !clip || track.locked) return p;
  const from = angles[angleOf(p, clip)];
  if (!from || from === target) return p;
  const fps = fpsValue(p.fps);
  // The same moment on the shared clock, then in the new camera's file.
  const moment = clip.source_in + (frame - clip.start) / fps + from.offset;
  const inTarget = moment - target.offset;
  if (inTarget < 0 || inTarget >= target.duration - 1 / fps) return p;
  const split = frame > clip.start ? splitAt(p, frame, clip.id) : p;
  const tracks = split.tracks.map((t) => {
    if (t.id !== MC_TRACK) return t;
    return {
      ...t,
      clips: t.clips.map((c) => {
        if (c.start !== frame) return c;
        // Cut short if this camera stops recording before the clip ends.
        const left = Math.floor((target.duration - inTarget) * fps);
        return { ...c, source: target.source, source_in: inTarget, duration: Math.max(1, Math.min(c.duration, left)), keyframes: [] };
      }),
    };
  });
  return mergeSame({ ...split, tracks });
}

/** Joins neighbouring clips of the same camera that continue each other. */
function mergeSame(p: Project): Project {
  const fps = fpsValue(p.fps);
  return {
    ...p,
    tracks: p.tracks.map((t) => {
      if (t.id !== MC_TRACK) return t;
      const out: Clip[] = [];
      for (const c of [...t.clips].sort((a, b) => a.start - b.start)) {
        const prev = out[out.length - 1];
        if (prev && prev.source === c.source && prev.start + prev.duration === c.start && Math.abs(prev.source_in + prev.duration / fps - c.source_in) < 0.5 / fps) {
          out[out.length - 1] = { ...prev, duration: prev.duration + c.duration };
        } else out.push(c);
      }
      return { ...t, clips: out };
    }),
  };
}
