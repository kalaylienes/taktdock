import { expect, test } from "@playwright/test";

import { TAP_RESET_MS, TapTempo } from "../src/lib/tap";
import { bpmToFill, fillToBpm, subdivisionLabel } from "../src/lib/utils";

test("tap tempo averages the last four intervals", () => {
  const t = new TapTempo();
  expect(t.tap(0)).toBeNull();
  expect(t.tap(500)).toBe(120);
  expect(t.tap(1000)).toBe(120);
  // One late tap moves the reading, but by a share of its error, not all of it.
  expect(t.tap(1700)).toBe(106);
  expect(t.tap(2200)).toBe(109);
  // Four intervals on, the late one has left the window.
  t.tap(2700);
  t.tap(3200);
  t.tap(3700);
  expect(t.tap(4200)).toBe(120);
});

test("a pause of two seconds starts a new tempo", () => {
  const t = new TapTempo();
  t.tap(0);
  t.tap(1000);
  expect(t.tap(1000 + TAP_RESET_MS + 1)).toBeNull();
  expect(t.tap(1000 + TAP_RESET_MS + 1 + 250)).toBe(240);
});

test("the slider maps both ends of the tempo range", () => {
  expect(bpmToFill(30)).toBe(0);
  expect(bpmToFill(300)).toBe(100);
  expect(fillToBpm(0)).toBe(30);
  expect(fillToBpm(1)).toBe(300);
  expect(fillToBpm(-1)).toBe(30);
  expect(fillToBpm(2)).toBe(300);
  for (let bpm = 30; bpm <= 300; bpm++) expect(fillToBpm(bpmToFill(bpm) / 100)).toBe(bpm);
});

test("the grid is named as a note value against the beat unit", () => {
  expect(subdivisionLabel(1, 4)).toBe("1/4");
  expect(subdivisionLabel(2, 4)).toBe("1/8");
  expect(subdivisionLabel(3, 4)).toBe("1/8T");
  expect(subdivisionLabel(4, 4)).toBe("1/16");
  expect(subdivisionLabel(1, 8)).toBe("1/8");
  expect(subdivisionLabel(4, 8)).toBe("1/32");
});
