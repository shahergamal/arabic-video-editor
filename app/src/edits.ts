// Timeline edits. Each returns a new project and never changes the old one,
// which is what makes undo safe.
import type { Clip, MediaKind, Project, Track } from "./types";
import { IDENTITY, newId } from "./types";
import { keyframesFrom, speedOf } from "./motion";

const fpsOf = (p: Project) => p.fps.num / p.fps.den;

function mapTracks(p: Project, f: (t: Track) => Track): Project {
  return { ...p, tracks: p.tracks.map(f) };
}

/** Splits every unlocked clip under `frame` (or only `clipId`) into two clips. */
export function splitAt(p: Project, frame: number, clipId?: string): Project {
  const fps = fpsOf(p);
  return mapTracks(p, (t) => {
    if (t.locked) return t;
    const clips: Clip[] = [];
    for (const c of t.clips) {
      if ((!clipId || c.id === clipId) && frame > c.start && frame < c.start + c.duration) {
        const first = frame - c.start;
        clips.push({ ...c, duration: first, fade_out: 0 });
        clips.push({
          ...c,
          id: newId(),
          start: frame,
          duration: c.duration - first,
          source_in: c.source_in + (first / fps) * speedOf(c),
          fade_in: 0,
          transition_in: null,
          keyframes: keyframesFrom(c, first),
        });
      } else clips.push(c);
    }
    return { ...t, clips };
  });
}

/** Removes a clip and pulls everything after it on the same track left. */
export function rippleDelete(p: Project, clipId: string): Project {
  const track = p.tracks.find((t) => t.clips.some((c) => c.id === clipId));
  if (!track || track.locked) return p;
  const gone = track.clips.find((c) => c.id === clipId)!;
  const end = gone.start + gone.duration;
  return mapTracks(p, (t) =>
    t.id !== track.id
      ? t
      : {
          ...t,
          clips: t.clips
            .filter((c) => c.id !== clipId)
            .map((c) => (c.start >= end ? { ...c, start: c.start - gone.duration } : c)),
        },
  );
}

/** Removes a clip and leaves its gap. */
export function liftDelete(p: Project, clipId: string): Project {
  return mapTracks(p, (t) => (t.locked ? t : { ...t, clips: t.clips.filter((c) => c.id !== clipId) }));
}

/**
 * Puts `clip` on `trackId`. Whatever was already under it on that track is
 * trimmed, split or removed, like "overwrite" in professional editors.
 */
export function place(p: Project, trackId: string, clip: Clip): Project {
  const fps = fpsOf(p);
  const s = clip.start;
  const e = clip.start + clip.duration;
  return mapTracks(p, (t) => {
    let clips = t.clips.filter((c) => c.id !== clip.id);
    if (t.id !== trackId) return clips.length === t.clips.length ? t : { ...t, clips };
    const out: Clip[] = [];
    for (const c of clips) {
      const cs = c.start;
      const ce = c.start + c.duration;
      if (ce <= s || cs >= e) out.push(c);
      else {
        if (cs < s) out.push({ ...c, duration: s - cs, fade_out: 0 });
        if (ce > e) {
          const cut = e - cs;
          out.push({ ...c, id: cs < s ? newId() : c.id, start: e, duration: ce - e, source_in: c.source_in + (cut / fps) * speedOf(c), fade_in: 0, transition_in: null, keyframes: keyframesFrom(c, cut) });
        }
      }
    }
    out.push(clip);
    out.sort((a, b) => a.start - b.start);
    return { ...t, clips: out };
  });
}

/** The end of the last clip on a track. */
export const trackEnd = (t: Track) => t.clips.reduce((m, c) => Math.max(m, c.start + c.duration), 0);

/** A new clip for a media file. Pictures last 5 seconds by default. */
export function makeClip(p: Project, source: string, media: MediaKind, seconds: number, start: number): Clip {
  const fps = fpsOf(p);
  const duration = Math.max(1, Math.floor((media === "image" ? 5 : seconds) * fps));
  return { id: newId(), source, start, duration, source_in: 0, volume: 1, media, transform: IDENTITY, fade_in: 0, fade_out: 0 };
}

/** Adds a clip at the end of the first track that fits its kind (adding a track if needed). */
export function appendMedia(p: Project, source: string, media: MediaKind, seconds: number): Project {
  const kind = media === "audio" ? "audio" : "video";
  let project = p;
  let track = project.tracks.find((t) => t.kind === kind && !t.locked);
  if (!track) {
    track = { id: newId(), kind, muted: false, hidden: false, locked: false, clips: [] };
    project = { ...project, tracks: [...project.tracks, track] };
  }
  return place(project, track.id, makeClip(project, source, media, seconds, trackEnd(track)));
}

/** Changes one clip, wherever it is. */
export function updateClip(p: Project, clip: Clip): Project {
  return mapTracks(p, (t) =>
    t.clips.some((c) => c.id === clip.id) ? { ...t, clips: t.clips.map((c) => (c.id === clip.id ? clip : c)) } : t,
  );
}

export function addTrack(p: Project, kind: "video" | "audio"): Project {
  const track: Track = { id: newId(), kind, muted: false, hidden: false, locked: false, clips: [] };
  // New video tracks go above existing ones; audio tracks go at the bottom.
  const lastVideo = p.tracks.map((t) => t.kind).lastIndexOf("video");
  const tracks = [...p.tracks];
  tracks.splice(kind === "video" ? lastVideo + 1 : tracks.length, 0, track);
  return { ...p, tracks };
}

export function removeTrack(p: Project, trackId: string): Project {
  const t = p.tracks.find((x) => x.id === trackId);
  if (!t || p.tracks.filter((x) => x.kind === t.kind).length <= 1) return p;
  return { ...p, tracks: p.tracks.filter((x) => x.id !== trackId) };
}

/**
 * Snaps a frame to the nearest edge (clip starts/ends, text starts/ends,
 * playhead, 0) within `tolerance` frames. Returns the frame unchanged if
 * nothing is close.
 */
export function snap(p: Project, frame: number, tolerance: number, playhead: number, ignoreId?: string): number {
  const edges = [0, playhead];
  for (const c of p.tracks.flatMap((t) => t.clips)) {
    if (c.id === ignoreId) continue;
    edges.push(c.start, c.start + c.duration);
  }
  for (const t of p.texts) {
    if (t.id === ignoreId) continue;
    edges.push(t.start, t.start + t.duration);
  }
  let best = frame;
  let dist = tolerance + 1;
  for (const e of edges) {
    const d = Math.abs(e - frame);
    if (d < dist) {
      dist = d;
      best = e;
    }
  }
  return dist <= tolerance ? best : frame;
}
