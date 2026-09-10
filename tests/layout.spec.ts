import { expect, test } from "@playwright/test";

import { boot, overflowing, requestedHeight, setAppearance } from "./harness";

/**
 * The window sizes come from two places: the floating height from what the
 * interface measures, the pinned strip from monitor.rs. These pin both to the
 * layout the plan drew, so a padding change that grows the widget by a pixel
 * is a failing test rather than a surprise on the taskbar.
 */

test("the floating widget asks for exactly two rows", async ({ page }) => {
  await boot(page);
  await expect.poll(() => requestedHeight(page)).toBe(50);
});

test("the expanded layout adds a second group of two rows", async ({ page }) => {
  await boot(page, {}, { expanded: true });
  await page.setViewportSize({ width: 300, height: 88 });
  await expect.poll(() => requestedHeight(page)).toBe(88);

  // And going back reports the smaller height again.
  await setAppearance(page, { expanded: false });
  await expect.poll(() => requestedHeight(page)).toBe(50);
});

test("the slots line up with the grid the layout was drawn on", async ({ page }) => {
  await boot(page);
  const boxes = await page.evaluate(() => {
    const first = document.querySelector(".td-row") as HTMLElement;
    const pick = (sel: string) => {
      const r = (first.querySelector(sel) as HTMLElement).getBoundingClientRect();
      return [Math.round(r.left), Math.round(r.right)];
    };
    return {
      mark: pick(".td-mark"),
      label: pick(".td-label"),
      slider: pick(".td-slot"),
      value: pick(".td-value"),
      tail: pick(".td-tail"),
    };
  });
  expect(boxes.mark).toEqual([9, 19]);
  expect(boxes.label).toEqual([25, 50]);
  expect(boxes.slider).toEqual([56, 193]);
  expect(boxes.value).toEqual([199, 247]);
  expect(boxes.tail).toEqual([253, 291]);
});

test("the grip exists only while floating", async ({ page }) => {
  await boot(page);
  await expect(page.locator(".td-grip")).toHaveCount(1);
  await setAppearance(page, { placement: "taskbar" });
  await expect(page.locator(".td-grip")).toHaveCount(0);
});

const STRIPS = [
  ["pinned", { placement: "taskbar" }, 208, 44],
  // A 36 pixel taskbar leaves a 32 pixel window once the margin is taken.
  ["pinned on a small taskbar", { placement: "taskbar" }, 208, 32],
  ["compact", { placement: "taskbar", compact: true }, 124, 44],
  ["compact on a small taskbar", { placement: "taskbar", compact: true }, 124, 32],
] as const;

for (const [name, appearance, w, h] of STRIPS) {
  test(`${name} fits ${w} x ${h} with the widest values`, async ({ page }) => {
    await boot(page, { bpm: 300, beats_per_bar: 7, beat_unit: 8, subdivision: 3 }, appearance);
    await page.setViewportSize({ width: w, height: h });
    await page.waitForTimeout(200);
    const out = await overflowing(page);
    expect(out, JSON.stringify(out)).toHaveLength(0);
  });
}

test("the pinned rows sit on the grid the plan drew", async ({ page }) => {
  await boot(page, {}, { placement: "taskbar" });
  await page.setViewportSize({ width: 208, height: 44 });
  const boxes = await page.evaluate(() => {
    const rows = Array.from(document.querySelectorAll(".td-row")) as HTMLElement[];
    const r = (el: Element) => {
      const b = el.getBoundingClientRect();
      return [Math.round(b.left), Math.round(b.right), Math.round(b.top)];
    };
    return {
      rows: rows.map((row) => Math.round(row.getBoundingClientRect().top)),
      play: r(rows[0].querySelector(".td-mark")!),
      meter: r(rows[0].querySelector(".td-label")!),
      dots: r(rows[0].querySelector(".td-slot")!),
      bpm: r(rows[0].querySelector(".td-value")!),
      slider: r(rows[1].querySelector(".td-slot")!),
    };
  });
  expect(boxes.play.slice(0, 2)).toEqual([7, 17]);
  expect(boxes.meter.slice(0, 2)).toEqual([21, 45]);
  expect(boxes.dots.slice(0, 2)).toEqual([49, 167]);
  expect(boxes.bpm.slice(0, 2)).toEqual([171, 201]);
  expect(boxes.slider.slice(0, 2)).toEqual([49, 167]);
  // Two rows of fifteen with two between, centred in the strip.
  expect(boxes.rows).toEqual([6, 23]);
});

test("every meter has a dot per beat", async ({ page }) => {
  await boot(page);
  for (const beats of [1, 2, 3, 4, 5, 6, 7]) {
    await page.evaluate((b) => {
      const td = (window as any).__TD;
      td.transport = { ...td.transport, beats_per_bar: b, beat_unit: b >= 6 ? 8 : 4 };
      td.emit("transport", td.transport);
    }, beats);
    await expect(page.locator(".td-dot")).toHaveCount(beats);
  }
});
