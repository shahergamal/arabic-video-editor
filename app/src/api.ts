import { invoke } from "@tauri-apps/api/core";
import type { FontInfo, MediaInfo, Project, Recent } from "./types";

export const api = {
  probe: (path: string) => invoke<MediaInfo>("probe_media", { path }),
  thumbnail: (path: string) => invoke<ArrayBuffer>("thumbnail", { path }),
  waveform: (path: string, buckets: number) => invoke<number[]>("waveform", { path, buckets }),
  fonts: () => invoke<FontInfo[]>("list_fonts"),
  addFont: (path: string) => invoke<void>("add_font", { path }),
  update: (project: Project) => invoke<void>("update_project", { project }),
  recover: () => invoke<Project | null>("recover_autosave"),
  discardAutosave: () => invoke<void>("discard_autosave"),
  recent: () => invoke<Recent[]>("list_recent"),
  forgetRecent: (path: string) => invoke<void>("forget_recent", { path }),
  save: (project: Project, path: string) => invoke<void>("save_project", { project, path }),
  open: (path: string) => invoke<Project>("open_project", { path }),
  missing: (project: Project) => invoke<string[]>("missing_media", { project }),
  frame: (index: number, width: number, height: number) =>
    invoke<ArrayBuffer>("render_frame", { index, width, height }),
  startExport: (path: string, scale: number, crf: number) =>
    invoke<void>("start_export", { request: { path, scale, crf } }),
  cancelExport: () => invoke<void>("cancel_export"),
};

/** Cached media info and waveforms, so the timeline asks only once per file. */
const infos = new Map<string, Promise<MediaInfo>>();
const waves = new Map<string, Promise<number[]>>();
const thumbs = new Map<string, Promise<string>>();

export const mediaInfo = (path: string) => {
  if (!infos.has(path)) infos.set(path, api.probe(path));
  return infos.get(path)!;
};
export const waveformOf = (path: string) => {
  if (!waves.has(path)) waves.set(path, api.waveform(path, 2000).catch(() => []));
  return waves.get(path)!;
};
export const thumbnailOf = (path: string) => {
  if (!thumbs.has(path))
    thumbs.set(
      path,
      api.thumbnail(path).then((b) => URL.createObjectURL(new Blob([new Uint8Array(b)], { type: "image/jpeg" }))),
    );
  return thumbs.get(path)!;
};
