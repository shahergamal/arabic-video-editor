// Mirrors crates/engine/src/project.rs. Keep the two in step.

export interface Fps {
  num: number;
  den: number;
}

export type MediaKind = "video" | "image" | "audio";
export type TrackKind = "video" | "audio";

export interface Transform {
  scale: number;
  x: number;
  y: number;
  opacity: number;
}

export interface Clip {
  id: string;
  source: string;
  start: number;
  duration: number;
  source_in: number;
  volume: number;
  media: MediaKind;
  transform: Transform;
  fade_in: number;
  fade_out: number;
  speed?: number;
  adjust?: Adjust;
  keyframes?: Keyframe[];
  transition_in?: Transition | null;
  remove_bg?: "people" | "objects" | null;
  chroma?: ChromaKey | null;
  mask?: Mask | null;
  speed_ramp?: SpeedPoint[];
  audio_fx?: AudioFx;
}

export interface ChromaKey {
  color: [number, number, number];
  strength: number;
  softness: number;
}

export interface MaskPoint {
  frame: number;
  x: number;
  y: number;
}

export interface Mask {
  shape: "rect" | "ellipse";
  mode: "keep" | "cut" | "blur";
  x: number;
  y: number;
  w: number;
  h: number;
  feather: number;
  blur: number;
  path?: MaskPoint[];
}

export interface SpeedPoint {
  frame: number;
  speed: number;
}

export interface AudioFx {
  denoise: boolean;
  voice: boolean;
}

export interface Adjust {
  brightness: number;
  contrast: number;
  saturation: number;
  temperature: number;
  shadows?: number;
  highlights?: number;
  tint?: number;
  vignette?: number;
  lut?: string | null;
  lut_strength?: number;
}

export interface Keyframe {
  /** Frames from the clip's start. */
  frame: number;
  transform: Transform;
}

export type TransitionKind = "dissolve" | "black" | "slide" | "wipe" | "zoom";
export interface Transition {
  kind: TransitionKind;
  frames: number;
}

export type TextAnimKind = "none" | "fade" | "rise" | "pop" | "zoom" | "words";
export interface TextAnim {
  kind: TextAnimKind;
  frames: number;
}

export const NEUTRAL: Adjust = { brightness: 0, contrast: 0, saturation: 0, temperature: 0 };

export interface Track {
  id: string;
  kind: TrackKind;
  muted: boolean;
  hidden: boolean;
  locked: boolean;
  duck?: boolean;
  clips: Clip[];
}

export type Rgba = [number, number, number, number];

export interface TextLayer {
  id: string;
  text: string;
  font_family: string;
  size: number;
  weight: number;
  color: Rgba;
  x: number;
  y: number;
  box_width: number;
  shadow: boolean;
  outline: number;
  outline_color: Rgba;
  background: Rgba;
  align: "center" | "right" | "left";
  start: number;
  duration: number;
  anim_in?: TextAnim | null;
  anim_out?: TextAnim | null;
}

export interface Project {
  version: number;
  name: string;
  width: number;
  height: number;
  fps: Fps;
  tracks: Track[];
  texts: TextLayer[];
  loudness?: number | null;
  angles?: Angle[];
}

/** One camera of a multi-camera shoot; see the engine's `Angle`. */
export interface Angle {
  source: string;
  /** Where this file starts on the first camera's clock, in seconds. */
  offset: number;
  duration: number;
  name: string;
}

export interface MediaInfo {
  width: number;
  height: number;
  duration: number;
  fps: Fps | null;
  has_audio: boolean;
  hdr?: "pq" | "hlg" | null;
}

export interface FontInfo {
  family: string;
  category: string;
  origin: "bundled" | "added" | "system";
  license: string;
  commercial_use: boolean | null;
  files: string[];
}

export interface Recent {
  path: string;
  name: string;
  opened: number;
}

export const fpsValue = (f: Fps) => f.num / f.den;

export const allClips = (p: Project) => p.tracks.flatMap((t) => t.clips);

export function projectDuration(p: Project): number {
  const ends = [...allClips(p).map((c) => c.start + c.duration), ...p.texts.map((t) => t.start + t.duration)];
  return ends.length ? Math.max(...ends) : 0;
}

export const newId = () => Math.random().toString(36).slice(2, 10);

export const IDENTITY: Transform = { scale: 1, x: 0, y: 0, opacity: 1 };

export const FORMATS = [
  { id: "9:16", label: "reels", width: 1080, height: 1920 },
  { id: "16:9", label: "youtube", width: 1920, height: 1080 },
  { id: "1:1", label: "square", width: 1080, height: 1080 },
  { id: "4:5", label: "post", width: 1080, height: 1350 },
] as const;

export function newProject(name: string, width: number, height: number): Project {
  return {
    version: 5,
    name,
    width,
    height,
    fps: { num: 30, den: 1 },
    tracks: [
      { id: "v1", kind: "video", muted: false, hidden: false, locked: false, clips: [] },
      { id: "a1", kind: "audio", muted: false, hidden: false, locked: false, clips: [] },
    ],
    texts: [],
  };
}

/** Projects saved by the first version kept clips outside tracks. */
export function normalize(p: Project & { clips?: Clip[] }): Project {
  const { clips, ...rest } = p;
  const project: Project = { ...rest, tracks: rest.tracks?.length ? rest.tracks : newProject("", 0, 0).tracks };
  if (clips?.length) {
    project.tracks = project.tracks.map((t, i) => (i === 0 ? { ...t, clips: [...t.clips, ...clips] } : t));
  }
  // Every project keeps at least one audio track for music and voice-over.
  if (!project.tracks.some((t) => t.kind === "audio"))
    project.tracks = [...project.tracks, { id: "a1", kind: "audio", muted: false, hidden: false, locked: false, clips: [] }];
  project.tracks = project.tracks.map((t) => ({
    ...t,
    clips: t.clips.map((c: Partial<Clip>) => ({
      ...c,
      media: c.media ?? mediaKindOf(c.source ?? ""),
      transform: c.transform ?? IDENTITY,
      fade_in: c.fade_in ?? 0,
      fade_out: c.fade_out ?? 0,
      volume: c.volume ?? 1,
    })) as Clip[],
  }));
  project.texts = project.texts.map((t: Partial<TextLayer>) => ({
    ...t,
    outline: t.outline ?? 0,
    outline_color: t.outline_color ?? [0, 0, 0, 255],
    background: t.background ?? [0, 0, 0, 0],
    align: t.align ?? "center",
  })) as TextLayer[];
  return project;
}

export function formatTime(frame: number, fps: Fps): string {
  const f = fpsValue(fps);
  const s = Math.floor(frame / f);
  const m = Math.floor(s / 60);
  const ff = Math.round(frame - s * f);
  return `${m}:${String(s % 60).padStart(2, "0")}.${String(ff).padStart(2, "0")}`;
}

export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export const VIDEO_EXT = ["mp4", "mov", "mkv", "webm", "m4v", "avi", "mts", "m2ts", "3gp"];
export const IMAGE_EXT = ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "heic"];
export const AUDIO_EXT = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "opus"];

export function mediaKindOf(path: string): MediaKind {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  if (IMAGE_EXT.includes(ext)) return "image";
  if (AUDIO_EXT.includes(ext)) return "audio";
  return "video";
}

/** Your brand: colours, the main font and a logo. */
export interface Brand {
  colors: string[];
  font: string | null;
  logo: string | null;
}
export const EMPTY_BRAND: Brand = { colors: [], font: null, logo: null };
