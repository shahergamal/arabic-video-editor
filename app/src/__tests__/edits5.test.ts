import { describe, expect, it } from "vitest";
import { addMarker, audibleClips, closeGaps, cutToBeats, detachAudio, duplicate, expand, freezeFrame, group, nextStop, paste, removeMarker, roll, setReverse, slip, ungroup } from "../edits5";
import { curveAt } from "../Phase5Clip";
import { fullCaption, targetProblems } from "../PublishDialog";
import type { Clip, Project } from "../types";
import { newProject } from "../types";

const clip = (id: string, start: number, duration: number, patch: Partial<Clip> = {}): Clip => ({
  id,
  source: `/m/${id}.mp4`,
  media: "video",
  start,
  duration,
  source_in: 0,
  volume: 1,
  transform: { scale: 1, x: 0, y: 0, opacity: 1 },
  fade_in: 0,
  fade_out: 0,
  ...patch,
});

function project(clips: Clip[], audio: Clip[] = []): Project {
  const p = newProject("t", 1080, 1920);
  const v = p.tracks.find((t) => t.kind === "video")!;
  const a = p.tracks.find((t) => t.kind === "audio")!;
  return { ...p, tracks: p.tracks.map((t) => (t.id === v.id ? { ...t, clips } : t.id === a.id ? { ...t, clips: audio } : t)) };
}
const videoClips = (p: Project) => p.tracks.find((t) => t.kind === "video")!.clips;
const end = (c: Clip) => c.start + c.duration;

describe("freeze frame", () => {
  it("splits the clip, holds the picture and pushes the rest later", () => {
    const p = project([clip("a", 0, 90, { source_in: 2 }), clip("b", 90, 30)]);
    const n = freezeFrame(p, "a", 30, 2);
    const v = videoClips(n);
    expect(v).toHaveLength(4);
    expect(v[0]).toMatchObject({ start: 0, duration: 30, source_in: 2 });
    expect(v[1]).toMatchObject({ start: 30, duration: 60, freeze: true, source_in: 3 });
    expect(v[2]).toMatchObject({ start: 90, duration: 60, source_in: 3 });
    expect(v[3]).toMatchObject({ id: "b", start: 150 });
  });

  it("ignores frames outside the clip", () => {
    const p = project([clip("a", 0, 30)]);
    expect(freezeFrame(p, "a", 40)).toBe(p);
  });
});

describe("reverse", () => {
  it("keeps the same part of the file when turned on and off", () => {
    const c = clip("a", 0, 60, { source_in: 1 }); // seconds 1..3 of a 10 s file
    const r = setReverse(c, true, 10, 30);
    expect(r.reverse).toBe(true);
    expect(r.source_in).toBeCloseTo(7); // 10 - (1 + 2)
    const back = setReverse(r, false, 10, 30);
    expect(back.source_in).toBeCloseTo(1);
  });
});

describe("duplicate, paste and gaps", () => {
  it("duplicates right after and moves the rest", () => {
    const n = duplicate(project([clip("a", 0, 30), clip("b", 30, 30)]), "a");
    const v = videoClips(n);
    expect(v.map((c) => c.start)).toEqual([0, 30, 60]);
    expect(v[1].source).toBe("/m/a.mp4");
    expect(v[1].id).not.toBe("a");
  });

  it("pastes at the playhead on a track of the right kind", () => {
    const n = paste(project([clip("a", 0, 30)]), clip("x", 0, 20, { media: "audio" }), 45);
    const audio = n.tracks.find((t) => t.kind === "audio")!.clips;
    expect(audio).toHaveLength(1);
    expect(audio[0].start).toBe(45);
  });

  it("closes gaps", () => {
    const p = project([clip("a", 10, 30), clip("b", 60, 30)]);
    const v = videoClips(closeGaps(p, p.tracks.find((t) => t.kind === "video")!.id));
    expect(v.map((c) => [c.start, c.duration])).toEqual([
      [0, 30],
      [30, 30],
    ]);
  });
});

describe("detach audio", () => {
  it("puts the sound on an audio track and silences the video", () => {
    const n = detachAudio(project([clip("a", 0, 30, { volume: 0.8 })]), "a");
    const v = videoClips(n)[0];
    const a = n.tracks.find((t) => t.kind === "audio")!.clips[0];
    expect(v.volume).toBe(0);
    expect(a).toMatchObject({ media: "audio", start: 0, duration: 30, volume: 0.8, source: "/m/a.mp4" });
  });

  it("makes a new audio track when the old one is busy", () => {
    const n = detachAudio(project([clip("a", 0, 30)], [clip("m", 0, 100, { media: "audio" })]), "a");
    expect(n.tracks.filter((t) => t.kind === "audio")).toHaveLength(2);
  });
});

describe("groups", () => {
  it("groups clips into one and takes them apart again", () => {
    const p = project([clip("a", 30, 30), clip("b", 60, 30)], [clip("m", 30, 60, { media: "audio" })]);
    const g = group(p, ["a", "b", "m"], "G");
    const v = videoClips(g);
    expect(v).toHaveLength(1);
    expect(v[0]).toMatchObject({ media: "compound", start: 30, duration: 60 });
    expect(g.compounds).toHaveLength(1);
    expect(g.tracks.find((t) => t.kind === "audio")!.clips).toHaveLength(0);
    // Inside, times start from the group's start.
    const parts = expand(g, v[0]);
    expect(parts.map((x) => [x.kind, x.clip.start, x.clip.duration])).toEqual([
      ["video", 30, 30],
      ["video", 60, 30],
      ["audio", 30, 60],
    ]);
    const back = ungroup(g, v[0].id);
    expect(videoClips(back).map((c) => [c.start, c.duration])).toEqual([
      [30, 30],
      [60, 30],
    ]);
    expect(back.tracks.find((t) => t.kind === "audio")!.clips[0]).toMatchObject({ start: 30, duration: 60 });
    expect(back.compounds).toHaveLength(0);
  });

  it("expands only the shown part of a trimmed group", () => {
    const p = project([clip("a", 0, 30), clip("b", 30, 30)]);
    const g = group(p, ["a", "b"], "G");
    const gc = { ...videoClips(g)[0], source_in: 0.5, duration: 30 }; // shows frames 15..45 of the group
    const parts = expand(g, gc);
    expect(parts.map((x) => [x.clip.start, x.clip.duration, x.clip.source_in])).toEqual([
      [0, 15, 0.5],
      [15, 15, 0],
    ]);
  });

  it("hears the clips inside groups but not backwards or frozen ones", () => {
    const p = project([clip("a", 0, 30), clip("b", 30, 30)]);
    const g = group(p, ["a", "b"], "G");
    expect(audibleClips(g).map((c) => c.source)).toEqual(["/m/a.mp4", "/m/b.mp4"]);
    expect(audibleClips(project([clip("r", 0, 30, { reverse: true }), clip("f", 30, 30, { freeze: true })]))).toHaveLength(0);
  });
});

describe("slip and roll", () => {
  it("slips inside the file", () => {
    const c = clip("a", 0, 60, { source_in: 1 });
    expect(slip(c, 30, 30, 10).source_in).toBeCloseTo(2);
    expect(slip(c, -300, 30, 10).source_in).toBe(0);
    expect(slip(c, 3000, 30, 10).source_in).toBeCloseTo(8);
  });

  it("rolls the cut between two clips", () => {
    const p = project([clip("a", 0, 30), clip("b", 30, 30, { source_in: 2 })]);
    const n = roll(p, "a", "b", 10, () => 10);
    const v = videoClips(n);
    expect(v.map((c) => [c.start, c.duration])).toEqual([
      [0, 40],
      [40, 20],
    ]);
    expect(v[1].source_in).toBeCloseTo(2 + 10 / 30);
    // b can't start before its file does: at most 60 frames back.
    const far = videoClips(roll(p, "a", "b", -100, () => 10));
    expect(far[0].duration).toBe(1);
  });
});

describe("markers", () => {
  it("adds, sorts, removes and jumps", () => {
    let p = project([clip("a", 0, 30), clip("b", 30, 30)]);
    p = addMarker(p, 50);
    p = addMarker(p, 10);
    p = addMarker(p, 10);
    expect(p.markers!.map((m) => m.frame)).toEqual([10, 50]);
    expect(nextStop(p, 0, true)).toBe(10);
    expect(nextStop(p, 10, true)).toBe(30);
    expect(nextStop(p, 55, false)).toBe(50);
    expect(removeMarker(p, 10).markers!.map((m) => m.frame)).toEqual([50]);
  });
});

describe("cut to beats", () => {
  it("ends every clip on a beat and never past its file", () => {
    const p = project([clip("a", 0, 40), clip("b", 40, 40), clip("c", 80, 40)]);
    const beats = [0, 15, 30, 45, 60, 75, 90, 105, 120];
    const n = cutToBeats(p, p.tracks.find((t) => t.kind === "video")!.id, beats, 1, (c) => (c.id === "b" ? 1 : 10));
    const v = videoClips(n);
    for (const c of v) expect(beats).toContain(end(c));
    expect(v[0].start).toBe(0);
    expect(v[1].start).toBe(end(v[0]));
    expect(v[1].duration).toBeLessThanOrEqual(30); // b's file is only 1 s long
  });
});

describe("curves", () => {
  it("draws a straight line for no change and passes through points", () => {
    expect(curveAt([[0, 0], [1, 1]], 0.3)).toBeCloseTo(0.3);
    expect(curveAt([[0, 0], [0.5, 0.7], [1, 1]], 0.5)).toBeCloseTo(0.7);
    // Never wiggles back.
    let last = -1;
    for (let x = 0; x <= 1; x += 0.05) {
      const y = curveAt([[0, 0], [0.3, 0.6], [0.6, 0.62], [1, 1]], x);
      expect(y).toBeGreaterThanOrEqual(last - 1e-6);
      last = y;
    }
  });
});

describe("publishing", () => {
  it("adds hashtags once each", () => {
    expect(fullCaption("شقة للبيع #عقارات", "عقارات, #القاهرة القاهرة")).toBe("شقة للبيع #عقارات\n\n#القاهرة");
    expect(fullCaption("", "")).toBe("");
  });

  it("finds what stops a post", () => {
    const wide = { ...project([clip("a", 0, 30 * 90)]), width: 1920, height: 1080 };
    const problems = targetProblems(wide, [
      { platform: "instagram", kind: "story", page_id: "" },
      { platform: "facebook", kind: "post", page_id: "" },
    ], "hi", false);
    expect(problems.map((p) => p.problem).sort()).toEqual(["needsTall", "storyTooLong"]);
    expect(targetProblems(wide, [{ platform: "tiktok", kind: "post", page_id: "" }], "x".repeat(2300), true).map((p) => p.problem)).toEqual(["captionTooLong"]);
  });
});
