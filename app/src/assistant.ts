// The assistant's helpers: cut quiet parts, check before publishing, and
// build a property listing from a few facts. Plain functions, so they can be
// tested without the app.
import type { Clip, Project, TextLayer } from "./types";
import { allClips, fpsValue, newId, projectDuration } from "./types";

/**
 * Removes the quiet stretches (in seconds of the file) from one clip and
 * pulls everything after it on the same track left. A little sound is kept
 * on each side of speech (`pad` seconds) so words are not clipped.
 */
export function cutSilences(p: Project, clipId: string, quiet: [number, number][], pad = 0.12, minKeep = 0.2): { project: Project; removed: number } {
  const fps = fpsValue(p.fps);
  const track = p.tracks.find((t) => t.clips.some((c) => c.id === clipId));
  const clip = track?.clips.find((c) => c.id === clipId);
  if (!track || !clip || track.locked || clip.speed_ramp?.length) return { project: p, removed: 0 };
  const speed = clip.speed ?? 1;
  const from = clip.source_in;
  const to = from + (clip.duration / fps) * speed;
  // Pieces of the file to keep.
  const keep: [number, number][] = [];
  let at = from;
  for (const [a0, b0] of [...quiet].sort((x, y) => x[0] - y[0])) {
    const a = Math.max(from, a0 + pad);
    const b = Math.min(to, b0 - pad);
    if (b <= a || b <= at) continue;
    if (a > at) keep.push([at, a]);
    at = Math.max(at, b);
  }
  if (to > at) keep.push([at, to]);
  const pieces: Clip[] = [];
  let start = clip.start;
  for (const [a, b] of keep) {
    const duration = Math.round(((b - a) / speed) * fps);
    if (b - a < minKeep || duration < 2) continue;
    pieces.push({ ...clip, id: pieces.length ? newId() : clip.id, start, duration, source_in: a, fade_in: 0, fade_out: 0, transition_in: pieces.length ? null : clip.transition_in, keyframes: [] });
    start += duration;
  }
  if (!pieces.length) return { project: p, removed: 0 };
  pieces[0].fade_in = clip.fade_in;
  pieces[pieces.length - 1].fade_out = clip.fade_out;
  const removed = clip.duration - (start - clip.start);
  if (removed <= 0) return { project: p, removed: 0 };
  const end = clip.start + clip.duration;
  const tracks = p.tracks.map((t) =>
    t.id !== track.id
      ? t
      : {
          ...t,
          clips: t.clips.flatMap((c) => (c.id === clipId ? pieces : c.start >= end ? [{ ...c, start: c.start - removed }] : [c])),
        },
  );
  return { project: { ...p, tracks }, removed };
}

export type CheckKey = "chkMissing" | "chkGap" | "chkSmallText" | "chkEdgeText" | "chkLong" | "chkLoud" | "chkEmpty" | "chkTextOver";
export interface CheckItem {
  key: CheckKey;
  /** Frame to jump to, when there is one place to look. */
  frame?: number;
  detail?: string;
}

/** Things worth fixing before the video goes out. */
export function publishCheck(p: Project, missing: string[]): CheckItem[] {
  const out: CheckItem[] = [];
  const fps = fpsValue(p.fps);
  const total = projectDuration(p);
  if (total === 0) return [{ key: "chkEmpty" }];
  if (missing.length) out.push({ key: "chkMissing", detail: String(missing.length) });

  // Frames where no picture is showing: black in the export.
  const pictures = p.tracks
    .filter((t) => t.kind === "video" && !t.hidden)
    .flatMap((t) => t.clips)
    .map((c) => [c.start, c.start + c.duration] as const)
    .sort((a, b) => a[0] - b[0]);
  let covered = 0;
  for (const [a, b] of pictures) {
    if (a > covered + 1) {
      out.push({ key: "chkGap", frame: covered, detail: `${(covered / fps).toFixed(1)}s` });
      break;
    }
    covered = Math.max(covered, b);
  }
  if (pictures.length && covered + 1 < total) out.push({ key: "chkGap", frame: covered, detail: `${(covered / fps).toFixed(1)}s` });

  const small = p.texts.find((t) => t.size < 0.028);
  if (small) out.push({ key: "chkSmallText", frame: small.start, detail: small.text.slice(0, 24) });

  // Tall videos: apps put buttons and captions near the bottom and the right.
  const tall = p.height > p.width;
  const edge = p.texts.find((t) => t.y < 0.06 || t.y > (tall ? 0.85 : 0.94) || t.x - t.box_width / 2 < -0.01 || t.x + t.box_width / 2 > 1.01);
  if (edge) out.push({ key: "chkEdgeText", frame: edge.start, detail: edge.text.slice(0, 24) });

  if (tall && total / fps > 90) out.push({ key: "chkLong", detail: `${Math.round(total / fps)}s` });

  const loudEnough = p.loudness != null;
  const hasSound = allClips(p).some((c) => c.media !== "image");
  if (hasSound && !loudEnough) out.push({ key: "chkLoud" });

  const after = p.texts.find((t) => t.start >= total);
  if (after) out.push({ key: "chkTextOver", frame: after.start, detail: after.text.slice(0, 24) });
  return out;
}

export interface Listing {
  kind: string;
  place: string;
  price: string;
  area: string;
  rooms: string;
  extra: string;
  phone: string;
}

type Style = Omit<TextLayer, "id" | "start" | "duration" | "text">;

/** A whole property video's words, spread over the project's length. */
export function listingTexts(p: Project, l: Listing, font: string, accent: [number, number, number, number]): TextLayer[] {
  const fps = fpsValue(p.fps);
  // Fit the words to the pictures; with no pictures yet, plan 12 seconds.
  const pictures = p.tracks.filter((t) => t.kind === "video").flatMap((t) => t.clips).reduce((m, c) => Math.max(m, c.start + c.duration), 0);
  const total = pictures > fps * 4 ? pictures : Math.round(12 * fps);
  const sec = (s: number) => Math.round(s * fps);
  const white: [number, number, number, number] = [255, 255, 255, 255];
  const base: Style = { font_family: font, size: 0.05, weight: 700, color: white, x: 0.5, y: 0.5, box_width: 0.88, shadow: true, outline: 0, outline_color: [0, 0, 0, 255], background: [0, 0, 0, 0], align: "center", anim_in: { kind: "rise", frames: sec(0.5) }, anim_out: { kind: "fade", frames: sec(0.3) } };
  const card: Style = { ...base, size: 0.038, y: 0.8, box_width: 0.8, shadow: false, background: [20, 20, 24, 210], anim_in: { kind: "pop", frames: sec(0.4) } };
  const out: TextLayer[] = [];
  const add = (text: string, style: Style, start: number, duration: number) => {
    if (text.trim()) out.push({ ...style, id: newId(), text: text.trim(), start, duration: Math.max(1, Math.min(duration, total - start)) });
  };
  const intro = sec(3);
  const outro = sec(3.5);
  add(l.kind, { ...base, size: 0.075, y: 0.42 }, 0, intro);
  add(l.place ? `📍 ${l.place}` : "", { ...base, size: 0.04, y: 0.52, color: accent, anim_in: { kind: "fade", frames: sec(0.6) } }, sec(0.3), intro - sec(0.3));

  const facts = [l.area && `${l.area} م²`, l.rooms && `${l.rooms} غرف`, ...l.extra.split(/[\n،,]+/)].filter((x): x is string => !!x && !!x.trim());
  const middle = Math.max(sec(1), total - intro - outro);
  const each = facts.length ? Math.floor(middle / facts.length) : 0;
  facts.forEach((f, i) => add(f, card, intro + i * each, each));

  const end = total - outro;
  add(l.price, { ...base, size: 0.07, y: 0.45, color: accent, anim_in: { kind: "zoom", frames: sec(0.5) } }, end, outro);
  add(l.phone ? `☎ ${l.phone}` : "", { ...card, y: 0.56, size: 0.045, background: [accent[0], accent[1], accent[2], 235] }, end + sec(0.4), outro - sec(0.4));
  return out;
}
