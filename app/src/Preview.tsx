import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "./api";
import { useT } from "./i18n";
import { audibleClips } from "./edits5";
import type { Clip, Project } from "./types";
import { fpsValue, projectDuration } from "./types";

interface Props {
  project: Project;
  /** Bumped whenever the project changed and the engine has the new copy. */
  revision: number;
  frame: number;
  playing: boolean;
  onFrame: (f: number) => void;
  onStop: () => void;
  /** Playing speed: 1 normal, 2 or 4 faster, negative plays backwards (J/K/L). */
  rate?: number;
}

/** Preview size: the project shape, at most 960 px on the long side, even numbers. */
function previewSize(p: Project, boxW: number, boxH: number) {
  const scale = Math.min(boxW / p.width, boxH / p.height, 960 / Math.max(p.width, p.height));
  const even = (n: number) => Math.max(2, Math.floor((n * scale) / 2) * 2);
  return { w: even(p.width), h: even(p.height) };
}

/** Clips you can hear, with their track's mute applied and groups taken apart. */
const audible = (p: Project): Clip[] => audibleClips(p);

/** Loudness of a clip at a frame, with its fades. The browser caps volume at 1. */
function gainAt(c: Clip, frame: number) {
  const into = frame - c.start;
  const left = c.start + c.duration - frame;
  let g = c.volume;
  if (c.fade_in > 0 && into < c.fade_in) g *= into / c.fade_in;
  if (c.fade_out > 0 && left < c.fade_out) g *= left / c.fade_out;
  return Math.max(0, Math.min(1, g));
}

export function Preview({ project, revision, frame, playing, onFrame, onStop, rate = 1 }: Props) {
  const t = useT();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [box, setBox] = useState({ w: 640, h: 360 });
  const [stats, setStats] = useState({ dropped: 0, fps: 0 });
  const { w, h } = previewSize(project, box.w, box.h);
  const live = useRef(project);
  live.current = project;

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth - 24, h: el.clientHeight - 24 }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Frames arrive as JPEG bytes; the browser decodes them off the main thread.
  const decode = (buf: ArrayBuffer) => createImageBitmap(new Blob([new Uint8Array(buf)], { type: "image/jpeg" }));
  const draw = (img: ImageBitmap) => {
    const ctx = canvas.current?.getContext("2d");
    if (ctx) ctx.drawImage(img, 0, 0, w, h);
    img.close();
  };

  // Paused: draw the frame under the playhead whenever something changes.
  useEffect(() => {
    if (playing) return;
    let alive = true;
    api
      .frame(frame, w, h)
      .then(decode)
      .then((img) => (alive ? draw(img) : img.close()))
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [frame, revision, w, h, playing]);

  // Playing: the computer clock drives the picture. Each audible clip gets
  // its own <audio> that is started, moved and faded to follow that clock.
  // If a frame takes too long, we skip ahead and count it.
  useEffect(() => {
    if (!playing) return;
    const p = live.current;
    const fps = fpsValue(p.fps);
    const end = projectDuration(p);
    const startFrame = rate > 0 && frame >= end - 1 ? 0 : frame;
    const t0 = performance.now();
    const players = new Map<string, HTMLAudioElement>();
    let stopped = false;
    let last = startFrame - 1;
    let shown = 0;
    let dropped = 0;

    const syncAudio = (target: number) => {
      // Backwards there is no sound, like scrubbing a tape.
      const now = rate < 0 ? [] : audible(live.current).filter((c) => target >= c.start && target < c.start + c.duration);
      for (const [id, el] of players) if (!now.some((c) => c.id === id)) el.pause();
      for (const c of now) {
        let el = players.get(c.id);
        if (!el) {
          el = new Audio(convertFileSrc(c.source));
          el.preload = "auto";
          players.set(c.id, el);
        }
        const speed = (c.speed ?? 1) * rate;
        const want = c.source_in + ((target - c.start) / fps) * (c.speed ?? 1);
        if (el.playbackRate !== speed) el.playbackRate = speed;
        if (el.paused || Math.abs(el.currentTime - want) > 0.2) {
          el.currentTime = want;
          el.play().catch(() => {});
        }
        el.volume = gainAt(c, target);
      }
    };

    const tick = async () => {
      while (!stopped) {
        const target = startFrame + Math.trunc(((performance.now() - t0) / 1000) * fps * rate);
        if (target >= end || target < 0) break;
        if (rate > 0 ? target <= last : target >= last && last !== startFrame - 1) {
          await new Promise((r) => requestAnimationFrame(() => r(null)));
          continue;
        }
        if (rate > 0 && target > last + 1 && last >= startFrame) dropped += target - last - 1;
        syncAudio(target);
        try {
          draw(await decode(await api.frame(target, w, h)));
        } catch {
          break;
        }
        last = target;
        shown++;
        onFrame(target);
        if (shown % 15 === 0) {
          const secs = (performance.now() - t0) / 1000;
          setStats({ dropped, fps: secs > 0 ? shown / secs : 0 });
        }
      }
      players.forEach((el) => el.pause());
      if (!stopped) onStop();
    };
    tick();
    return () => {
      stopped = true;
      players.forEach((el) => {
        el.pause();
        el.src = "";
      });
    };
    // Restart only when play is pressed or the picture size changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, w, h, rate]);

  return (
    <div className="preview" ref={wrap}>
      <canvas ref={canvas} width={w} height={h} style={{ width: w, height: h }} />
      {stats.fps > 0 && (
        <div className="stats">
          {stats.fps.toFixed(1)} {t("frames")} · {t("droppedFrames")}: {stats.dropped}
        </div>
      )}
    </div>
  );
}
