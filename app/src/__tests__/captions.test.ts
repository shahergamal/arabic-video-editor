import { describe, expect, it } from "vitest";
import { CAPTION_PREFIX, segmentsToTexts } from "../captions";
import type { TextLayer } from "../types";

const style = {
  font_family: "Cairo", size: 0.04, weight: 700, color: [255, 255, 255, 255], x: 0.5, y: 0.8, box_width: 0.9,
  shadow: false, outline: 0.06, outline_color: [0, 0, 0, 255], background: [0, 0, 0, 0], align: "center",
} as Omit<TextLayer, "id" | "start" | "duration" | "text">;

describe("segmentsToTexts", () => {
  it("places lines at their times, in frames", () => {
    const t = segmentsToTexts([{ start: 1, end: 2.5, text: " أهلاً بيكم " }], 30, style);
    expect(t).toHaveLength(1);
    expect(t[0]).toMatchObject({ start: 30, duration: 45, text: "أهلاً بيكم" });
    expect(t[0].id.startsWith(CAPTION_PREFIX)).toBe(true);
  });

  it("never lets two lines show at once", () => {
    const t = segmentsToTexts(
      [
        { start: 0, end: 2, text: "السطر الأول" },
        { start: 1.5, end: 3, text: "السطر التاني" },
      ],
      30,
      style,
    );
    expect(t[0].start + t[0].duration).toBeLessThanOrEqual(t[1].start);
  });

  it("keeps very short lines readable and drops empty ones", () => {
    const t = segmentsToTexts(
      [
        { start: 0, end: 0.1, text: "لا" },
        { start: 5, end: 6, text: "   " },
      ],
      30,
      style,
    );
    expect(t).toHaveLength(1);
    expect(t[0].duration).toBeGreaterThanOrEqual(18);
  });
});
