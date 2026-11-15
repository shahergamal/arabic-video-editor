import { invoke } from "@tauri-apps/api/core";
import type { MediaInfo, Project } from "./types";

export const api = {
  probe: (path: string) => invoke<MediaInfo>("probe_media", { path }),
  fonts: () => invoke<string[]>("list_fonts"),
  update: (project: Project) => invoke<void>("update_project", { project }),
  recover: () => invoke<Project | null>("recover_autosave"),
  save: (project: Project, path: string) => invoke<void>("save_project", { project, path }),
  open: (path: string) => invoke<Project>("open_project", { path }),
  frame: (index: number, width: number, height: number) =>
    invoke<ArrayBuffer>("render_frame", { index, width, height }),
  startExport: (path: string) => invoke<void>("start_export", { path }),
  cancelExport: () => invoke<void>("cancel_export"),
};
