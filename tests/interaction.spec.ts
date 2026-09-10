import { expect, test } from "@playwright/test";

import { boot, calls, clearCalls, setAppearance } from "./harness";

test("play asks the backend to toggle and shows the new state at once", async ({ page }) => {
  await boot(page);
  await page.locator(".td-play").click();
  expect(await calls(page, "toggle_play")).toHaveLength(1);
  await expect(page.locator(".td-play")).toHaveAttribute("data-running", "true");
});

test("the wheel nudges the tempo by one, and by five with shift", async ({ page }) => {
  await boot(page);
  await page.mouse.move(150, 25);
  await clearCalls(page);

  await page.mouse.wheel(0, -100);
  await page.mouse.wheel(0, 100);
  await page.keyboard.down("Shift");
  await page.mouse.wheel(0, -100);
  await page.keyboard.up("Shift");

  await expect
    .poll(async () => (await calls(page, "nudge_tempo")).map((c) => c.args.delta))
    .toEqual([1, -1, 5]);
});

test("small touchpad wheel events add up to a step", async ({ page }) => {
  await boot(page);
  await page.mouse.move(150, 25);
  await clearCalls(page);
  for (let i = 0; i < 5; i++) await page.mouse.wheel(0, -12);
  await expect.poll(async () => (await calls(page, "nudge_tempo")).length).toBe(1);
});

test("dragging the slider sets the tempo under the pointer", async ({ page }) => {
  await boot(page);
  const slider = page.locator(".td-slider");
  const box = (await slider.boundingBox())!;
  await clearCalls(page);

  // The left edge is the slowest tempo and the right edge the fastest.
  await page.mouse.move(box.x + 1, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 5 });
  await page.mouse.move(box.x + box.width + 20, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();

  const sent = (await calls(page, "set_tempo")).map((c) => c.args.bpm as number);
  expect(sent[0]).toBeLessThanOrEqual(32);
  expect(sent).toContain(165);
  expect(sent[sent.length - 1]).toBe(300);
  // Only whole tempos that changed are sent, never a flood of repeats.
  for (let i = 1; i < sent.length; i++) expect(sent[i]).not.toBe(sent[i - 1]);
});

test("tapping four times sets the tempo from the intervals", async ({ page }) => {
  // Paused, so the only time that passes between taps is the time given.
  await page.clock.install({ time: new Date("2026-09-01T10:00:00Z") });
  await boot(page);
  await page.clock.pauseAt(new Date("2026-09-01T10:00:05Z"));
  const tap = page.locator(".td-link", { hasText: "tap" });
  for (let i = 0; i < 4; i++) {
    await tap.click();
    await page.clock.runFor(500);
  }
  const sent = (await calls(page, "set_tempo")).map((c) => c.args.bpm);
  expect(sent[sent.length - 1]).toBe(120);
});

test("a pill steps to the next value on click and lists them on right click", async ({ page }) => {
  await boot(page);
  await clearCalls(page);

  await page.locator('[data-pill="meter"]').click();
  await page.locator('[data-pill="subdivision"]').click();
  expect(await calls(page, "cycle_meter")).toHaveLength(1);
  expect(await calls(page, "cycle_subdivision")).toHaveLength(1);

  await page.locator('[data-pill="meter"]').click({ button: "right" });
  const menus = await calls(page, "show_pill_menu");
  expect(menus.map((c) => c.args.pill)).toEqual(["meter"]);
  // The pill has a menu of its own, and it does not also open the full one.
  expect(await calls(page, "show_context_menu")).toHaveLength(0);
});

test("a right click anywhere else opens the full menu", async ({ page }) => {
  await boot(page);
  await clearCalls(page);
  await page.locator(".td-bpm").click({ button: "right" });
  expect(await calls(page, "show_context_menu")).toHaveLength(1);
});

test("the expanded layout switches the sound and scrolls the volume", async ({ page }) => {
  await boot(page, {}, { expanded: true });
  await page.setViewportSize({ width: 300, height: 88 });
  await clearCalls(page);

  await page.locator(".td-pill", { hasText: "click" }).click();
  expect(await calls(page, "cycle_sound")).toHaveLength(1);

  const volume = page.locator(".td-value", { hasText: "75%" });
  await volume.hover();
  await page.mouse.wheel(0, 100);
  await expect
    .poll(async () => (await calls(page, "nudge_volume")).map((c) => c.args.delta))
    .toEqual([-5]);
  // Scrolling the volume is not also a tempo change.
  expect(await calls(page, "nudge_tempo")).toHaveLength(0);
});

test("pinned, the tap link and the slider are on the second row", async ({ page }) => {
  await boot(page, {}, { placement: "taskbar" });
  await page.setViewportSize({ width: 208, height: 44 });
  await setAppearance(page, {});
  const rows = page.locator(".td-row");
  await expect(rows.nth(1).locator(".td-link")).toHaveText("tap");
  await expect(rows.nth(1).locator(".td-slider")).toHaveCount(1);
});
