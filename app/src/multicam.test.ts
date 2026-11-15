import { describe, expect, it } from "vitest";
import { MC_TRACK, buildMulticam, switchAngle } from "./multicam";
import { newProject, type Angle } from "./types";

const angles: Angle[] = [
  { source: "/a.mp4", offset: 0, duration: 20, name: "" },
  { source: "/b.mp4", offset: 2, duration: 20, name: "" },
  { source: "/c.mp4", offset: -1, duration: 10, name: "" },
];

describe("multicam", () => {
  const p = buildMulticam(newProject("t", 1920, 1080), angles);
  const mc = () => p.tracks.find((t) => t.id === MC_TRACK)!;

  it("covers the time every camera was recording, with sound from the first", () => {
    // From 2 s (b starts) to 9 s (c ends) on a's clock.
    expect(mc().clips[0]).toMatchObject({ source: "/a.mp4", source_in: 2, start: 0, duration: 210 });
    expect(mc().muted).toBe(true);
    const sound = p.tracks.find((t) => t.kind === "audio")!.clips[0];
    expect(sound).toMatchObject({ source: "/a.mp4", media: "audio", source_in: 2, duration: 210 });
    expect(p.angles![1].name).toBe("2 · b.mp4");
  });

  it("switches camera at the same moment and joins it back", () => {
    let q = switchAngle(p, 60, 1);
    let clips = q.tracks.find((t) => t.id === MC_TRACK)!.clips;
    expect(clips.map((c) => c.source)).toEqual(["/a.mp4", "/b.mp4"]);
    // 2 s into the cut is 4 s on a's clock, 2 s into b's file.
    expect(clips[1].source_in).toBeCloseTo(2);
    q = switchAngle(q, 120, 2);
    clips = q.tracks.find((t) => t.id === MC_TRACK)!.clips;
    // 4 s in: 6 s on a's clock, 7 s into c's file; c ends at 10 s, 3 s later.
    expect(clips[2]).toMatchObject({ source: "/c.mp4", start: 120, duration: 90 });
    expect(clips[2].source_in).toBeCloseTo(7);
    // Choosing the camera already showing changes nothing.
    q = switchAngle(q, 90, 0);
    expect(switchAngle(q, 150, 2)).toBe(q);
    expect(switchAngle(q, 100, 0)).toBe(q);
  });

  it("joins neighbouring pieces of the same camera", () => {
    const q = switchAngle(switchAngle(p, 60, 1), 60, 0);
    const clips = q.tracks.find((t) => t.id === MC_TRACK)!.clips;
    expect(clips.length).toBe(1);
    expect(clips[0]).toMatchObject({ source: "/a.mp4", duration: 210 });
  });

  it("refuses a camera that was not recording then", () => {
    const late = buildMulticam(newProject("t", 1920, 1080), [angles[0], { source: "/d.mp4", offset: 5, duration: 3, name: "" }]);
    // The shared part is 5..8 s.
    expect(late.tracks.find((t) => t.id === MC_TRACK)!.clips[0].duration).toBe(90);
    const p2 = buildMulticam(newProject("t", 1920, 1080), [{ ...angles[0], duration: 30 }, { source: "/d.mp4", offset: 5, duration: 3, name: "" }]);
    const cut = switchAngle(p2, 0, 1);
    expect(cut.tracks.find((t) => t.id === MC_TRACK)!.clips[0].source).toBe("/d.mp4");
    // 4 s in, d has stopped (it ran 5..8 s): the switch is refused.
    expect(switchAngle(p2, 120, 1)).toBe(p2);
  });
});
