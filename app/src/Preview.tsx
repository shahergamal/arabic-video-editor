import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { api } from "./api";
import type { Project } from "./types";
import { fpsValue, projectDuration } from "./types";

interface Props {
  project: Project;
  /** Bumped whenever the project changed and the engine has the new copy. */
  revision: number;
  frame: number;
  playing: boolean;
  onFrame: (f: number) => void;
  onStop: () => void;
}

/** Preview size: the project shape, at most 960 px wide, even numbers. */
function previewSize(p: Project, boxW: number, boxH: number) {
  const scale = Math.min(boxW / p.width, boxH / p.height, 960 / Math.max(p.width, p.height));
  const even = (n: number) => Math.max(2, Math.floor((n * scale) / 2) * 2);
  return { w: even(p.width), h: even(p.height) };
}

export function Preview({ project, revision, frame, playing, onFrame, onStop }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const audio = useRef<HTMLAudioElement>(null);
  const [box, setBox] = useState({ w: 640, h: 360 });
  const [stats, setStats] = useState({ shown: 0, dropped: 0, fps: 0, fetchMs: 0, drawMs: 0 });
  const { w, h } = previewSize(project, box.w, box.h);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setBox({ w: el.clientWidth - 24, h: el.clientHeight - 24 }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Frames arrive as JPEG bytes; the browser decodes them off the main thread.
  const decode = (buf: ArrayBuffer | ArrayLike<number>) => {
    const bytes = buf instanceof ArrayBuffer ? new Uint8Array(buf) : Uint8Array.from(buf as ArrayLike<number>);
    return createImageBitmap(new Blob([bytes], { type: "image/jpeg" }));
  };
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
  }, [frame, revision, w, h, playing]);

  // Playing: the clock is the sound when there is sound, otherwise the
  // computer clock. If a frame takes too long, we skip ahead and count it.
  useEffect(() => {
    if (!playing) return;
    const fps = fpsValue(project.fps);
    const end = projectDuration(project);
    const startFrame = frame >= end - 1 ? 0 : frame;
    const t0 = performance.now();
    const clip = project.clips.find((c) => startFrame >= c.start && startFrame < c.start + c.duration);
    const a = audio.current;
    let audioClock = false;
    if (a && clip) {
      a.src = convertFileSrc(clip.source);
      a.currentTime = clip.source_in + (startFrame - clip.start) / fps;
      a.volume = Math.min(1, clip.volume);
      audioClock = true;
      a.play().catch(() => (audioClock = false));
    }
    let stopped = false;
    let last = startFrame - 1;
    let shown = 0;
    let dropped = 0;
    let fetchMs = 0;
    let drawMs = 0;
    const began = performance.now();

    const tick = async () => {
      while (!stopped) {
        let target: number;
        if (audioClock && a && clip && !a.paused) {
          target = clip.start + Math.floor((a.currentTime - clip.source_in) * fps);
          if (target >= clip.start + clip.duration) audioClock = false;
        } else {
          target = startFrame + Math.floor(((performance.now() - t0) / 1000) * fps);
        }
        if (target >= end) break;
        if (target <= last) {
          await new Promise((r) => requestAnimationFrame(() => r(null)));
          continue;
        }
        if (target > last + 1) dropped += target - last - 1;
        try {
          const t1 = performance.now();
          const img = await decode(await api.frame(target, w, h));
          const t2 = performance.now();
          draw(img);
          fetchMs += t2 - t1;
          drawMs += performance.now() - t2;
        } catch {
          break;
        }
        last = target;
        shown++;
        onFrame(target);
        const secs = (performance.now() - began) / 1000;
        if (shown % 10 === 0) setStats({ shown, dropped, fps: secs > 0 ? shown / secs : 0, fetchMs: fetchMs / shown, drawMs: drawMs / shown });
      }
      a?.pause();
      if (!stopped) onStop();
    };
    tick();
    return () => {
      stopped = true;
      a?.pause();
    };
    // Restart only when play is pressed or the picture size changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, w, h]);

  return (
    <div className="preview" ref={wrap}>
      <canvas ref={canvas} width={w} height={h} style={{ width: w, height: h }} />
      <audio ref={audio} preload="auto" />
      <div className="stats" title="سرعة المعاينة أثناء التشغيل">
        {stats.fps.toFixed(1)} إطار/ث · إطارات ناقصة: {stats.dropped} · جلب {stats.fetchMs.toFixed(0)}ms · رسم {stats.drawMs.toFixed(0)}ms
      </div>
    </div>
  );
}
