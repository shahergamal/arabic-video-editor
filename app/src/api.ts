import { invoke } from "@tauri-apps/api/core";
import type { Brand, FontInfo, MediaInfo, Project, Recent } from "./types";

export interface ExportJob {
  path: string;
  width: number;
  height: number;
  crf: number;
  fill?: boolean;
  /** "auto" uses the graphics card when it works, "cpu" never does. */
  encoder?: "auto" | "cpu";
}

export interface TemplateInfo {
  name: string;
  path: string;
  width: number;
  height: number;
  texts: number;
}

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
  startExport: (jobs: ExportJob[]) => invoke<void>("start_export", { jobs }),
  saveTemplate: (project: Project, name: string) => invoke<void>("save_template", { project, name }),
  templates: () => invoke<TemplateInfo[]>("list_templates"),
  deleteTemplate: (path: string) => invoke<void>("delete_template", { path }),
  loadTemplate: (path: string) => invoke<Project>("load_template", { path }),
  matteModels: () => invoke<{ id: string; mb: number; downloaded: boolean }[]>("matte_models"),
  downloadMatteModel: (id: string) => invoke<void>("download_matte_model", { id }),
  prepareMatte: (clipId: string) => invoke<void>("prepare_matte", { clipId }),
  trackMask: (clipId: string, from: number, x: number, y: number, w: number, h: number) =>
    invoke<void>("track_mask", { clipId, from, x, y, w, h }),
  scopes: (index: number) => invoke<ArrayBuffer>("scopes", { index }),
  silences: (path: string, threshold: number, minSeconds: number) =>
    invoke<[number, number][]>("silences", { path, threshold, minSeconds }),
  syncAngles: (paths: string[]) => invoke<{ source: string; offset: number; confidence: number }[]>("sync_angles", { paths }),
  exportTimeline: (path: string, format: "otio" | "xml" | "edl") => invoke<string[]>("export_timeline", { path, format }),
  importTimeline: (path: string) => invoke<Project>("import_timeline", { path }),
  plugins: () => invoke<Plugin[]>("list_plugins"),
  installPlugin: (folder: string) => invoke<string>("install_plugin", { folder }),
  removePlugin: (id: string) => invoke<void>("remove_plugin", { id }),
  brand: () => invoke<Brand | null>("get_brand"),
  setBrand: (brand: Brand) => invoke<void>("set_brand", { brand }),
  cancelExport: () => invoke<void>("cancel_export"),
  captionModels: () => invoke<{ id: string; mb: number; downloaded: boolean }[]>("caption_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  transcribe: (model: string, language: string) => invoke<void>("transcribe", { model, language }),
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

export interface PluginLook {
  name: string;
  name_ar?: string | null;
  adjust: Partial<Record<string, number>>;
  lut?: string | null;
}
export interface PluginText {
  name: string;
  name_ar?: string | null;
  style: Record<string, unknown>;
}
export interface Plugin {
  id: string;
  name: string;
  name_ar?: string | null;
  version: string;
  author: string;
  license: string;
  looks: PluginLook[];
  texts: PluginText[];
  fonts: string[];
  lut_paths: (string | null)[];
}
