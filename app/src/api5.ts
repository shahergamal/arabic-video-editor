// Phase 5 calls into the app: working copies, beats, reframe, recording,
// updates, the automatic edit and publishing.
import { invoke } from "@tauri-apps/api/core";
import type { Project } from "./types";

export interface Beats {
  tempo: number;
  times: number[];
}
export interface Moment {
  at: number;
  sharp: number;
  bright: number;
  motion: number;
  shake: number;
}
export interface ShotInfo {
  path: string;
  kind: "video" | "image" | "audio";
  duration: number;
  width: number;
  height: number;
  has_audio: boolean;
  moments: Moment[];
  speech: string;
}
export interface Brief {
  idea: string;
  width: number;
  height: number;
  seconds: number;
  pace: "" | "fast" | "calm";
  font: string;
  accent?: [number, number, number, number];
}
export type Planner = "rules" | "local" | "claude";
export interface AiView {
  planner: Planner;
  has_key: boolean;
  monthly_cap: number;
  spent: number;
  local_ready: boolean;
  local_mb: number;
}
export interface PlanResult {
  project: Project;
  planner: Planner;
  note: string;
  cost: number;
}
export interface Update {
  version: string;
  url: string;
}
export interface SocialView {
  meta_app: boolean;
  meta_name: string;
  pages: { id: string; name: string; ig_username: string }[];
  tiktok_app: boolean;
  tiktok_name: string;
  redirect: string;
}
export type Platform = "facebook" | "instagram" | "tiktok";
export type PostKind = "post" | "reel" | "story";
export interface Target {
  platform: Platform;
  kind: PostKind;
  page_id: string;
}
export interface PublishJob {
  file: string;
  vertical_file?: string | null;
  caption: string;
  title: string;
  targets: Target[];
  tiktok_privacy: string;
  tiktok_allow_comments: boolean;
}
export interface Outcome {
  target: Target;
  ok: boolean;
  message: string;
}

export const api5 = {
  prepareCopy: (clipId: string) => invoke<void>("prepare_copy", { clipId }),
  copyReady: (clipId: string) => invoke<boolean>("copy_ready", { clipId }),
  detectBeats: (path: string, start: number, length: number) => invoke<Beats>("detect_beats", { path, start, length }),
  encoders: () => invoke<string[]>("encoders"),
  autoReframe: (width: number, height: number, model: string) => invoke<void>("auto_reframe", { width, height, model }),
  /** The recording goes as the raw request body, not as JSON. */
  saveRecording: (bytes: Uint8Array) => invoke<string>("save_recording", bytes),
  checkUpdate: () => invoke<Update | null>("check_update"),
  openReleasePage: (url: string) => invoke<void>("open_release_page", { url }),
  aiSettings: () => invoke<AiView>("ai_settings"),
  setAiSettings: (planner: Planner, monthlyCap: number, claudeKey?: string) => invoke<void>("set_ai_settings", { planner, monthlyCap, claudeKey: claudeKey ?? null }),
  analyzeShots: (paths: string[]) => invoke<void>("analyze_shots", { paths }),
  estimatePlan: (brief: Brief, shots: ShotInfo[]) => invoke<{ dollars: number; spent: number; cap: number }>("estimate_plan", { brief, shots }),
  makeEdit: (brief: Brief, shots: ShotInfo[], name: string) => invoke<void>("make_edit", { brief, shots, name }),
  downloadLocalAi: () => invoke<void>("download_local_ai"),
  transcribe: (model: string, language: string, translate: boolean) => invoke<void>("transcribe", { model, language, translate }),
  socialStatus: () => invoke<SocialView>("social_status"),
  socialSetApps: (platform: "meta" | "tiktok", id: string, secret: string) => invoke<SocialView>("social_set_apps", { platform, id, secret }),
  socialConnect: (platform: "meta" | "tiktok") => invoke<void>("social_connect", { platform }),
  socialDisconnect: (platform: "meta" | "tiktok") => invoke<SocialView>("social_disconnect", { platform }),
  tiktokPrivacy: () => invoke<string[]>("tiktok_privacy_options"),
  publish: (job: PublishJob) => invoke<void>("publish", { job }),
  openPlatform: (platform: Platform | "meta-dev" | "tiktok-dev") => invoke<void>("open_platform", { platform }),
};
