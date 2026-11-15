import { describe, expect, it } from "vitest";
import { addTrack, appendMedia, makeClip, place, rippleDelete, snap, splitAt } from "../edits";
import { newProject, projectDuration } from "../types";

const base = () => appendMedia(appendMedia(newProject("t", 1080, 1920), "a.mp4", "video", 2), "b.mp4", "video", 3);

describe("timeline edits", () => {
  it("appends clips one after another on the video track", () => {
    const p = base();
    const [a, b] = p.tracks[0].clips;
    expect(a.start).toBe(0);
    expect(a.duration).toBe(60);
    expect(b.start).toBe(60);
    expect(projectDuration(p)).toBe(150);
  });

  it("music goes on the audio track, pictures last 5 seconds", () => {
    let p = appendMedia(base(), "song.mp3", "audio", 10);
    p = appendMedia(p, "logo.png", "image", 0);
    expect(p.tracks[1].clips[0].source).toBe("song.mp3");
    expect(p.tracks[0].clips.at(-1)!.duration).toBe(150);
  });

  it("split keeps the picture continuous", () => {
    const p = splitAt(base(), 30);
    const [a, b] = p.tracks[0].clips;
    expect(a.duration).toBe(30);
    expect(b.start).toBe(30);
    expect(b.source_in).toBeCloseTo(1);
    expect(b.duration).toBe(30);
  });

  it("ripple delete closes the gap", () => {
    const p0 = base();
    const p = rippleDelete(p0, p0.tracks[0].clips[0].id);
    expect(p.tracks[0].clips).toHaveLength(1);
    expect(p.tracks[0].clips[0].start).toBe(0);
  });

  it("placing a clip over others overwrites just that part", () => {
    const p0 = base();
    const c = makeClip(p0, "c.mp4", "video", 1, 45);
    const p = place(p0, p0.tracks[0].id, c);
    const clips = p.tracks[0].clips;
    expect(clips.map((x) => [x.source, x.start, x.duration])).toEqual([
      ["a.mp4", 0, 45],
      ["c.mp4", 45, 30],
      ["b.mp4", 75, 75],
    ]);
    expect(clips[2].source_in).toBeCloseTo(0.5);
  });

  it("a clip dropped inside a long clip splits it around", () => {
    const p0 = appendMedia(newProject("t", 1080, 1920), "long.mp4", "video", 10);
    const p = place(p0, p0.tracks[0].id, makeClip(p0, "x.mp4", "video", 1, 60));
    expect(p.tracks[0].clips.map((x) => [x.source, x.start, x.duration])).toEqual([
      ["long.mp4", 0, 60],
      ["x.mp4", 60, 30],
      ["long.mp4", 90, 210],
    ]);
  });

  it("locked tracks are not split", () => {
    const p0 = base();
    p0.tracks[0].locked = true;
    expect(splitAt(p0, 30)).toEqual(p0);
  });

  it("new video tracks go above video, audio tracks at the bottom", () => {
    let p = addTrack(newProject("t", 1, 1), "video");
    p = addTrack(p, "audio");
    expect(p.tracks.map((t) => t.kind)).toEqual(["video", "video", "audio", "audio"]);
  });

  it("snaps to nearby edges only", () => {
    const p = base();
    expect(snap(p, 62, 3, 200)).toBe(60);
    expect(snap(p, 66, 3, 200)).toBe(66);
    expect(snap(p, 198, 3, 200)).toBe(200);
  });
});
