// Mirrors Clip::transform_at in crates/engine/src/project.rs.
import type { Clip, Keyframe, Transform } from "./types";

export const speedOf = (c: Clip) => c.speed ?? 1;

const ease = (t: number) => t * t * (3 - 2 * t);

/** Position and size of a clip at timeline frame `frame`. */
export function transformAt(c: Clip, frame: number): Transform {
  const keys = [...(c.keyframes ?? [])].sort((a, b) => a.frame - b.frame);
  if (!keys.length) return c.transform;
  const local = Math.max(0, frame - c.start);
  if (local <= keys[0].frame) return keys[0].transform;
  const last = keys[keys.length - 1];
  if (local >= last.frame) return last.transform;
  const i = keys.findIndex((k) => k.frame > local);
  const a = keys[i - 1];
  const b = keys[i];
  const e = ease((local - a.frame) / Math.max(1, b.frame - a.frame));
  const mix = (x: number, y: number) => x + (y - x) * e;
  return {
    scale: mix(a.transform.scale, b.transform.scale),
    x: mix(a.transform.x, b.transform.x),
    y: mix(a.transform.y, b.transform.y),
    opacity: mix(a.transform.opacity, b.transform.opacity),
    rotation: mix(a.transform.rotation ?? 0, b.transform.rotation ?? 0),
  };
}

/** Sets the clip's look at `frame`: edits that keyframe, adds one, or (with no keyframes) the fixed transform. */
export function setTransformAt(c: Clip, frame: number, t: Transform): Clip {
  if (!c.keyframes?.length) return { ...c, transform: t };
  const local = frame - c.start;
  const others = c.keyframes.filter((k) => k.frame !== local);
  return { ...c, keyframes: [...others, { frame: local, transform: t }].sort((a, b) => a.frame - b.frame) };
}

/** Keyframes for the part of a clip that starts `offset` frames in. */
export function keyframesFrom(c: Clip, offset: number): Keyframe[] | undefined {
  if (!c.keyframes?.length) return c.keyframes;
  const at = transformAt(c, c.start + offset);
  return [{ frame: 0, transform: at }, ...c.keyframes.filter((k) => k.frame > offset).map((k) => ({ ...k, frame: k.frame - offset }))];
}

/** Ready-made moves, using the clip's own length. */
export type MovePreset = "zoomIn" | "zoomOut" | "panRight" | "panLeft";
export function movePreset(c: Clip, kind: MovePreset): Keyframe[] {
  const end = Math.max(1, c.duration - 1);
  const base = c.transform;
  const k = (frame: number, patch: Partial<Transform>): Keyframe => ({ frame, transform: { ...base, ...patch } });
  switch (kind) {
    case "zoomIn":
      return [k(0, { scale: base.scale }), k(end, { scale: base.scale * 1.2 })];
    case "zoomOut":
      return [k(0, { scale: base.scale * 1.2 }), k(end, { scale: base.scale })];
    case "panRight":
      return [k(0, { scale: base.scale * 1.2, x: -0.08 }), k(end, { scale: base.scale * 1.2, x: 0.08 })];
    case "panLeft":
      return [k(0, { scale: base.scale * 1.2, x: 0.08 }), k(end, { scale: base.scale * 1.2, x: -0.08 })];
  }
}
