import { describe, expect, it } from "vitest";
import { cutSilences, listingTexts, publishCheck } from "./assistant";
import { newProject, type Clip, type Project } from "./types";

const clip = (over: Partial<Clip> = {}): Clip =>
  ({ id: "c", source: "a.mp4", media: "video", start: 0, duration: 300, source_in: 0, volume: 1, fade_in: 0, fade_out: 0, transform: { scale: 1, x: 0, y: 0, opacity: 1 }, ...over }) as Clip;
const withClips = (clips: Clip[]): Project => {
  const p = newProject("t", 1080, 1920);
  p.tracks[0].clips = clips;
  return p;
};

describe("cutSilences", () => {
  it("removes the quiet parts and pulls later clips left", () => {
    const p = withClips([clip(), clip({ id: "d", start: 300 })]);
    // 10 s clip at 30 fps; quiet from 2 to 4 s and from 8 to 9 s.
    const { project, removed } = cutSilences(p, "c", [
      [2, 4],
      [8, 9],
    ]);
    const cs = project.tracks[0].clips;
    expect(cs.length).toBe(4);
    expect(cs[0]).toMatchObject({ id: "c", start: 0, source_in: 0 });
    expect(cs[1].source_in).toBeCloseTo(3.88);
    expect(cs[1].start).toBe(cs[0].duration);
    // (2 - 0.24) + (1 - 0.24) seconds of quiet go.
    expect(removed / 30).toBeCloseTo(2.52, 1);
    expect(cs[3]).toMatchObject({ id: "d", start: 300 - removed });
  });
  it("leaves ramped clips alone", () => {
    const p = withClips([clip({ speed_ramp: [{ frame: 0, speed: 1 }, { frame: 299, speed: 2 }] })]);
    expect(cutSilences(p, "c", [[1, 3]]).removed).toBe(0);
  });
});

describe("publishCheck", () => {
  it("finds gaps, small text and text near the bottom of tall videos", () => {
    const p = withClips([clip({ duration: 60 }), clip({ id: "d", start: 90, duration: 60 })]);
    p.texts.push({ id: "t", text: "صغير", font_family: "Cairo", size: 0.02, weight: 400, color: [255, 255, 255, 255], x: 0.5, y: 0.9, box_width: 0.8, shadow: false, outline: 0, outline_color: [0, 0, 0, 255], background: [0, 0, 0, 0], align: "center", start: 0, duration: 30 });
    const keys = publishCheck(p, []).map((c) => c.key);
    expect(keys).toContain("chkGap");
    expect(keys).toContain("chkSmallText");
    expect(keys).toContain("chkEdgeText");
    expect(keys).toContain("chkLoud");
    p.loudness = -14;
    expect(publishCheck(p, []).map((c) => c.key)).not.toContain("chkLoud");
  });
});

describe("listingTexts", () => {
  it("spreads the facts between the title and the price", () => {
    const p = withClips([clip({ duration: 600 })]);
    const t = listingTexts(p, { kind: "شقة للبيع", place: "التجمع", price: "٣ مليون", area: "150", rooms: "3", extra: "جراج، أسانسير", phone: "0100" }, "Cairo", [232, 163, 61, 255]);
    expect(t.map((x) => x.text)).toEqual(["شقة للبيع", "📍 التجمع", "150 م²", "3 غرف", "جراج", "أسانسير", "٣ مليون", "☎ 0100"]);
    expect(Math.max(...t.map((x) => x.start + x.duration))).toBeLessThanOrEqual(600);
    const facts = t.slice(2, 6);
    facts.forEach((f, i) => i && expect(f.start).toBeGreaterThanOrEqual(facts[i - 1].start + facts[i - 1].duration));
  });
});
