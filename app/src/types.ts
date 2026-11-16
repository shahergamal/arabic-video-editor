// Mirrors crates/engine/src/project.rs. Keep the two in step.

export interface Fps {
  num: number;
  den: number;
}

export interface Clip {
  id: string;
  source: string;
  start: number;
  duration: number;
  source_in: number;
  volume: number;
}

export interface TextLayer {
  id: string;
  text: string;
  font_family: string;
  size: number;
  weight: number;
  color: [number, number, number, number];
  x: number;
  y: number;
  box_width: number;
  shadow: boolean;
  start: number;
  duration: number;
}

export interface Project {
  version: number;
  name: string;
  width: number;
  height: number;
  fps: Fps;
  clips: Clip[];
  texts: TextLayer[];
}

export interface MediaInfo {
  width: number;
  height: number;
  duration: number;
  fps: Fps | null;
  has_audio: boolean;
}

export const fpsValue = (f: Fps) => f.num / f.den;

export function projectDuration(p: Project): number {
  const ends = [...p.clips.map((c) => c.start + c.duration), ...p.texts.map((t) => t.start + t.duration)];
  return ends.length ? Math.max(...ends) : 0;
}

export const newId = () => Math.random().toString(36).slice(2, 10);

export const FORMATS = [
  { id: "9:16", label: "ريلز / تيك توك", width: 1080, height: 1920 },
  { id: "16:9", label: "يوتيوب", width: 1920, height: 1080 },
  { id: "1:1", label: "مربع", width: 1080, height: 1080 },
  { id: "4:5", label: "منشور", width: 1080, height: 1350 },
] as const;

export function formatTime(frame: number, fps: Fps): string {
  const s = frame / fpsValue(fps);
  const m = Math.floor(s / 60);
  const sec = Math.floor(s % 60);
  const ff = Math.round(frame % fpsValue(fps));
  return `${m}:${String(sec).padStart(2, "0")}:${String(ff).padStart(2, "0")}`;
}
