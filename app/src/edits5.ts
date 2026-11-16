// Phase 5 timeline edits: freeze frame, reverse, duplicate, paste, close
// gaps, detach sound, groups, slip and roll, markers and cutting to the
// beat. Like edits.ts, each returns a new project and never changes the old.
import type { Clip, Compound, Marker, Project, Track, TrackKind } from "./types";
import { newId } from "./types";
import { keyframesFrom, speedOf, transformAt } from "./motion";
import { place } from "./edits";

const fpsOf = (p: Project) => p.fps.num / p.fps.den;
const trackOf = (p: Project, clipId: string) => p.tracks.find((t) => t.clips.some((c) => c.id === clipId));
const clipOf = (p: Project, clipId: string) => p.tracks.flatMap((t) => t.clips).find((c) => c.id === clipId);
const withTrack = (p: Project, t: Track): Project => ({ ...p, tracks: p.tracks.map((x) => (x.id === t.id ? t : x)) });

/** Moves every clip on a track that starts at or after `from` by `by` frames. */
function shiftAfter(t: Track, from: number, by: number, except?: string): Track {
  return { ...t, clips: t.clips.map((c) => (c.id !== except && c.start >= from ? { ...c, start: c.start + by } : c)) };
}

/** Seconds into the source shown at timeline frame `frame` (no ramps). */
export function sourceTimeAt(c: Clip, frame: number, fps: number): number {
  if (c.freeze) return c.source_in;
  return c.source_in + ((frame - c.start) / fps) * speedOf(c);
}

/**
 * Holds the picture under the playhead for `seconds`: the clip is split
 * there and a still of that moment is put in between, pushing the rest of
 * the track later.
 */
export function freezeFrame(p: Project, clipId: string, frame: number, seconds = 2): Project {
  const t = trackOf(p, clipId);
  const c = clipOf(p, clipId);
  if (!t || !c || t.locked || c.media !== "video" || frame < c.start || frame >= c.start + c.duration) return p;
  const fps = fpsOf(p);
  const len = Math.max(1, Math.round(seconds * fps));
  const into = frame - c.start;
  const at = sourceTimeAt(c, frame, fps);
  const still: Clip = {
    ...c,
    id: newId(),
    start: frame,
    duration: len,
    source_in: at,
    freeze: true,
    speed: 1,
    speed_ramp: [],
    fade_in: 0,
    fade_out: 0,
    transition_in: null,
    transform: transformAt(c, frame),
    keyframes: [],
    mask: c.mask ? { ...c.mask, path: [] } : c.mask,
  };
  const clips: Clip[] = [];
  for (const x of t.clips) {
    if (x.id !== c.id) {
      clips.push(x.start >= frame ? { ...x, start: x.start + len } : x);
      continue;
    }
    if (into > 0) clips.push({ ...c, duration: into, fade_out: 0 });
    clips.push(still);
    clips.push({
      ...c,
      id: into > 0 ? newId() : c.id,
      start: frame + len,
      duration: c.duration - into,
      source_in: at,
      fade_in: 0,
      transition_in: null,
      keyframes: keyframesFrom(c, into),
    });
  }
  clips.sort((a, b) => a.start - b.start);
  return withTrack(p, { ...t, clips });
}

/**
 * Turns playing backwards on or off. The same part of the file stays in
 * use: `source_in` is converted between the normal and the reversed file
 * (which is `fileSeconds` long).
 */
export function setReverse(c: Clip, on: boolean, fileSeconds: number, fps: number): Clip {
  if (!!c.reverse === on) return c;
  const span = (c.duration / fps) * speedOf(c);
  const source_in = Math.max(0, Math.min(fileSeconds - span, fileSeconds - (c.source_in + span)));
  return { ...c, reverse: on, source_in, speed_ramp: [], mask: c.mask ? { ...c.mask, path: [] } : c.mask };
}

/** A copy of the clip right after it; the rest of the track moves along. */
export function duplicate(p: Project, clipId: string): Project {
  const t = trackOf(p, clipId);
  const c = clipOf(p, clipId);
  if (!t || !c || t.locked) return p;
  const end = c.start + c.duration;
  const copy = { ...c, id: newId(), start: end, transition_in: null };
  const moved = shiftAfter(t, end, c.duration, c.id);
  return withTrack(p, { ...moved, clips: [...moved.clips, copy].sort((a, b) => a.start - b.start) });
}

/** Puts a copied clip at `frame` on a track of the right kind. */
export function paste(p: Project, clip: Clip, frame: number, preferTrack?: string): Project {
  const kind: TrackKind = clip.media === "audio" ? "audio" : "video";
  const target =
    p.tracks.find((t) => t.id === preferTrack && t.kind === kind && !t.locked) ?? p.tracks.find((t) => t.kind === kind && !t.locked);
  if (!target) return p;
  return place(p, target.id, { ...clip, id: newId(), start: frame, transition_in: null });
}

/** Pulls every clip on the track left so nothing is left between them. */
export function closeGaps(p: Project, trackId: string): Project {
  const t = p.tracks.find((x) => x.id === trackId);
  if (!t || t.locked) return p;
  let at = 0;
  const clips = [...t.clips]
    .sort((a, b) => a.start - b.start)
    .map((c) => {
      const out = { ...c, start: at };
      at += c.duration;
      return out;
    });
  return withTrack(p, { ...t, clips });
}

/** The video's sound as its own clip on an audio track, so it can be moved and cut on its own. */
export function detachAudio(p: Project, clipId: string): Project {
  const c = clipOf(p, clipId);
  if (!c || c.media !== "video" || c.freeze || c.speed_ramp?.length) return p;
  const s = c.start;
  const e = c.start + c.duration;
  let tracks = p.tracks;
  let target = tracks.find((t) => t.kind === "audio" && !t.locked && t.clips.every((x) => x.start + x.duration <= s || x.start >= e));
  if (!target) {
    target = { id: newId(), kind: "audio", muted: false, hidden: false, locked: false, clips: [] };
    tracks = [...tracks, target];
  }
  const sound: Clip = {
    ...c,
    id: newId(),
    media: "audio",
    transform: { scale: 1, x: 0, y: 0, opacity: 1 },
    keyframes: [],
    transition_in: null,
    adjust: undefined,
    mask: null,
    chroma: null,
    remove_bg: null,
    crop: undefined,
    flip_h: false,
    flip_v: false,
    blend: "",
  };
  const tid = target.id;
  return {
    ...p,
    tracks: tracks.map((t) =>
      t.id === tid
        ? { ...t, clips: [...t.clips, sound].sort((a, b) => a.start - b.start) }
        : { ...t, clips: t.clips.map((x) => (x.id === c.id ? { ...x, volume: 0, audio_fx: undefined } : x)) },
    ),
  };
}

/** Groups clips into one compound clip, placed on the first picked track. */
export function group(p: Project, ids: string[], name: string): Project {
  const picked = p.tracks.flatMap((t) => t.clips.filter((c) => ids.includes(c.id)).map((c) => ({ t, c })));
  if (picked.length < 2 || picked.some(({ t }) => t.locked)) return p;
  const start = Math.min(...picked.map(({ c }) => c.start));
  const end = Math.max(...picked.map(({ c }) => c.start + c.duration));
  const groupId = `group-${newId()}`;
  const used = p.tracks.filter((t) => t.clips.some((c) => ids.includes(c.id)));
  const inner: Track[] = used.map((t) => ({
    ...t,
    id: newId(),
    duck: t.duck,
    clips: t.clips.filter((c) => ids.includes(c.id)).map((c) => ({ ...c, start: c.start - start })),
  }));
  const compound: Compound = { id: groupId, name, tracks: inner };
  const home = used.find((t) => t.kind === "video") ?? used[0];
  const clip: Clip = {
    id: newId(),
    source: groupId,
    media: "compound",
    start,
    duration: end - start,
    source_in: 0,
    volume: 1,
    transform: { scale: 1, x: 0, y: 0, opacity: 1 },
    fade_in: 0,
    fade_out: 0,
  };
  const without: Project = {
    ...p,
    tracks: p.tracks.map((t) => ({ ...t, clips: t.clips.filter((c) => !ids.includes(c.id)) })),
    compounds: [...(p.compounds ?? []), compound],
  };
  return place(without, home.id, clip);
}

/**
 * The clips inside a group clip, cut to the part it shows and placed where
 * they play. Mirrors Project::expand in the engine.
 */
export function expand(p: Project, c: Clip): { kind: TrackKind; ti: number; muted: boolean; clip: Clip }[] {
  const g = (p.compounds ?? []).find((x) => x.id === c.source);
  if (!g) return [];
  const fps = fpsOf(p);
  const from = Math.max(0, Math.round(c.source_in * fps));
  const to = from + c.duration;
  const out: { kind: TrackKind; ti: number; muted: boolean; clip: Clip }[] = [];
  g.tracks.forEach((t, ti) => {
    for (const inner of t.clips) {
      const s = Math.max(inner.start, from);
      const e = Math.min(inner.start + inner.duration, to);
      if (s >= e) continue;
      const cut = s - inner.start;
      const rate = inner.media === "compound" || inner.freeze ? 1 : speedOf(inner);
      const n: Clip = {
        ...inner,
        id: `${c.id}/${inner.id}`,
        start: c.start + (s - from),
        duration: e - s,
        source_in: inner.freeze ? inner.source_in : inner.source_in + (cut / fps) * rate,
        volume: inner.volume * c.volume,
        fade_in: cut > 0 ? 0 : inner.fade_in,
        fade_out: e < inner.start + inner.duration ? 0 : inner.fade_out,
        transition_in: cut > 0 ? null : inner.transition_in,
        keyframes: cut > 0 ? keyframesFrom(inner, cut) : inner.keyframes,
      };
      out.push({ kind: t.kind, ti, muted: t.muted, clip: n });
    }
  });
  return out;
}

/** Takes a group clip apart again: its clips go back onto the timeline. */
export function ungroup(p: Project, clipId: string): Project {
  const t = trackOf(p, clipId);
  const c = clipOf(p, clipId);
  if (!t || !c || c.media !== "compound" || t.locked) return p;
  let project: Project = { ...p, tracks: p.tracks.map((x) => ({ ...x, clips: x.clips.filter((y) => y.id !== c.id) })) };
  const parts = expand(p, c);
  // Each inner track goes to a free track of its kind, starting with the group's own.
  const dest = new Map<string, string>();
  for (const part of parts) {
    const key = `${part.kind}:${part.ti}`;
    if (!dest.has(key)) {
      const busy = (tr: Track) => parts.filter((q) => `${q.kind}:${q.ti}` === key).some(({ clip }) => tr.clips.some((x) => x.start < clip.start + clip.duration && clip.start < x.start + x.duration));
      const taken = new Set(dest.values());
      let tr = project.tracks.find((x) => x.kind === part.kind && !x.locked && !taken.has(x.id) && (x.id === t.id || !busy(x)) && !busy(x));
      if (!tr) {
        tr = { id: newId(), kind: part.kind, muted: false, hidden: false, locked: false, clips: [] };
        const tracks = [...project.tracks];
        const lastVideo = tracks.map((x) => x.kind).lastIndexOf("video");
        tracks.splice(part.kind === "video" ? lastVideo + 1 : tracks.length, 0, tr);
        project = { ...project, tracks };
      }
      dest.set(key, tr.id);
    }
    const id = dest.get(key)!;
    const clip = { ...part.clip, id: newId() };
    project = { ...project, tracks: project.tracks.map((x) => (x.id === id ? { ...x, clips: [...x.clips, clip].sort((a, b) => a.start - b.start) } : x)) };
  }
  const stillUsed = project.tracks.some((x) => x.clips.some((y) => y.media === "compound" && y.source === c.source));
  if (!stillUsed) project = { ...project, compounds: (project.compounds ?? []).filter((g) => g.id !== c.source) };
  return project;
}

/** Shows an earlier or later part of the file without moving the clip ("slip"). */
export function slip(c: Clip, frames: number, fps: number, fileSeconds: number): Clip {
  if (c.media === "image" || c.freeze || c.media === "compound") return c;
  const span = (c.duration / fps) * speedOf(c);
  const source_in = Math.max(0, Math.min(Math.max(0, fileSeconds - span), c.source_in + (frames / fps) * speedOf(c)));
  return { ...c, source_in };
}

/**
 * Moves the cut between two clips that touch ("roll"): one gets longer,
 * the other shorter by the same amount, so nothing else moves.
 */
export function roll(p: Project, leftId: string, rightId: string, frames: number, fileSeconds: (c: Clip) => number): Project {
  const t = trackOf(p, leftId);
  const a = clipOf(p, leftId);
  const b = clipOf(p, rightId);
  if (!t || !a || !b || t.locked || a.start + a.duration !== b.start) return p;
  const fps = fpsOf(p);
  // Limits: each clip keeps at least one frame and stays inside its file.
  const aMax = a.media === "image" || a.freeze ? Infinity : Math.floor(((fileSeconds(a) - a.source_in) * fps) / speedOf(a)) - a.duration;
  const bMax = b.media === "image" || b.freeze ? Infinity : Math.floor((b.source_in * fps) / speedOf(b));
  const d = Math.max(-(a.duration - 1), -bMax, Math.min(frames, b.duration - 1, aMax));
  if (!d) return p;
  const clips = t.clips.map((c) => {
    if (c.id === a.id) return { ...a, duration: a.duration + d, fade_out: Math.min(a.fade_out, a.duration + d) };
    if (c.id === b.id)
      return {
        ...b,
        start: b.start + d,
        duration: b.duration - d,
        source_in: b.media === "image" || b.freeze ? b.source_in : b.source_in + (d / fps) * speedOf(b),
        keyframes: d > 0 ? keyframesFrom(b, d) : b.keyframes,
      };
    return c;
  });
  return withTrack(p, { ...t, clips });
}

export const MARKER_COLORS = ["#f5a524", "#3ea6ff", "#4cd07d", "#ff5d73", "#b38cff"];

export function addMarker(p: Project, frame: number, label = ""): Project {
  const markers = p.markers ?? [];
  if (markers.some((m) => m.frame === frame)) return p;
  const m: Marker = { frame, label, color: MARKER_COLORS[markers.length % MARKER_COLORS.length] };
  return { ...p, markers: [...markers, m].sort((a, b) => a.frame - b.frame) };
}

export function removeMarker(p: Project, frame: number): Project {
  return { ...p, markers: (p.markers ?? []).filter((m) => m.frame !== frame) };
}

/** The next (or previous) marker or cut from `frame`, for jumping around. */
export function nextStop(p: Project, frame: number, forward: boolean): number {
  const stops = new Set<number>([0]);
  for (const m of p.markers ?? []) stops.add(m.frame);
  for (const c of p.tracks.flatMap((t) => t.clips)) {
    stops.add(c.start);
    stops.add(c.start + c.duration);
  }
  const list = [...stops].sort((a, b) => a - b);
  return forward ? (list.find((f) => f > frame) ?? frame) : ([...list].reverse().find((f) => f < frame) ?? 0);
}

/**
 * Retimes the clips of a track one after another so every cut falls on a
 * beat (`beats`, timeline frames), using every `every`-th beat. A clip
 * never plays past the end of its file.
 */
export function cutToBeats(p: Project, trackId: string, beats: number[], every: number, fileSeconds: (c: Clip) => number): Project {
  const t = p.tracks.find((x) => x.id === trackId);
  if (!t || t.locked || beats.length < 2) return p;
  const fps = fpsOf(p);
  const grid = beats.filter((_, i) => i % Math.max(1, every) === 0);
  const clips = [...t.clips].sort((a, b) => a.start - b.start);
  if (!clips.length) return p;
  let at = clips[0].start;
  const out: Clip[] = [];
  for (const c of clips) {
    const room = c.media === "image" || c.freeze ? Infinity : Math.floor(((fileSeconds(c) - c.source_in) * fps) / speedOf(c));
    const wanted = at + c.duration;
    // The beat nearest to where the clip would end, not before its start and not past its file.
    const ok = grid.filter((g) => g > at && g - at <= room);
    let end = ok.length ? ok.reduce((best, g) => (Math.abs(g - wanted) < Math.abs(best - wanted) ? g : best), ok[0]) : at + Math.min(c.duration, room);
    if (end <= at) end = at + 1;
    out.push({ ...c, start: at, duration: end - at, fade_out: Math.min(c.fade_out, end - at), transition_in: c.transition_in && c.transition_in.frames < end - at ? c.transition_in : null });
    at = end;
  }
  return withTrack(p, { ...t, clips: out });
}

/** Every clip heard in the preview, groups taken apart (mirrors the engine). */
export function audibleClips(p: Project): Clip[] {
  const out: Clip[] = [];
  const visit = (c: Clip, depth: number) => {
    if (c.media === "compound") {
      if (depth > 8) return;
      for (const part of expand(p, c)) if (!part.muted) visit(part.clip, depth + 1);
      return;
    }
    if (c.media !== "image" && c.volume > 0 && !c.speed_ramp?.length && !c.freeze && !c.reverse) out.push(c);
  };
  for (const t of p.tracks.filter((x) => !x.muted)) for (const c of t.clips) visit(c, 0);
  return out;
}
