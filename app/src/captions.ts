// Turns what the speech model heard into caption texts on the timeline.
import type { TextLayer } from "./types";

export interface Segment {
  start: number;
  end: number;
  text: string;
}

/** Caption texts get ids starting with this, so a new run can replace them. */
export const CAPTION_PREFIX = "cap_";

export function segmentsToTexts(
  segments: Segment[],
  fps: number,
  style: Omit<TextLayer, "id" | "start" | "duration" | "text">,
): TextLayer[] {
  const minFrames = Math.round(fps * 0.6);
  const sorted = segments
    .map((s) => ({ ...s, text: s.text.replace(/\s+/g, " ").trim() }))
    .filter((s) => s.text && s.end > s.start)
    .sort((a, b) => a.start - b.start);
  const out: TextLayer[] = [];
  sorted.forEach((s, i) => {
    const start = Math.max(0, Math.round(s.start * fps));
    let end = Math.max(start + minFrames, Math.round(s.end * fps));
    // Never overlap the next line: one caption on screen at a time.
    const next = sorted[i + 1];
    if (next) end = Math.min(end, Math.max(start + 1, Math.round(next.start * fps)));
    const prev = out[out.length - 1];
    const from = prev ? Math.max(start, prev.start + prev.duration) : start;
    if (end <= from) return;
    out.push({ ...style, id: `${CAPTION_PREFIX}${i}_${Math.random().toString(36).slice(2, 7)}`, text: s.text, start: from, duration: end - from });
  });
  return out;
}
