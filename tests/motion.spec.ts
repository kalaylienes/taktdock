import { expect, test } from "@playwright/test";

import { beat, boot, setAppearance, setTransport } from "./harness";

test("a beat lights its dot, the downbeat in the core tone", async ({ page }) => {
  await boot(page, { running: true });
  await beat(page, { beat: 0, kind: "accent" });
  await expect(page.locator('.td-dot[data-beat="0"] .td-dot-lit')).toHaveAttribute(
    "data-kind",
    "accent",
  );

  await beat(page, { beat: 2, kind: "beat" });
  await expect(page.locator('.td-dot[data-beat="2"] .td-dot-lit')).toHaveAttribute(
    "data-kind",
    "beat",
  );
  await expect(page.locator(".td-dot-lit")).toHaveCount(1);
});

test("clicks between beats do not move the dots", async ({ page }) => {
  await boot(page, { running: true, subdivision: 4 });
  await beat(page, { beat: 1, kind: "beat" });
  await beat(page, { beat: 1, sub: 2, kind: "sub" });
  await expect(page.locator('.td-dot[data-beat="1"] .td-dot-lit')).toHaveCount(1);
});

test("a dot waits for the moment its click is heard", async ({ page }) => {
  await boot(page, { running: true });
  await beat(page, { beat: 3, kind: "beat" }, 150);
  await expect(page.locator(".td-dot-lit")).toHaveCount(0);
  await expect(page.locator('.td-dot[data-beat="3"] .td-dot-lit')).toHaveCount(1, {
    timeout: 1000,
  });
});

test("the lit dot fades while motion is allowed", async ({ page }) => {
  await boot(page, { running: true, bpm: 60 });
  await beat(page, { beat: 0, kind: "accent", bpm: 60 });
  const lit = page.locator(".td-dot-lit");
  await expect(lit).toHaveCount(1);
  await page.waitForTimeout(700);
  const opacity = await lit.evaluate((el) => parseFloat(getComputedStyle(el).opacity));
  expect(opacity).toBeLessThan(0.6);
});

/**
 * The picture stops for a game, for battery saver, or when asked to. The beat
 * events keep coming the whole time, because the sound never stopped, and the
 * dots still follow them: they just change colour instead of fading.
 */
test("with motion off the dots still follow the beat, without fading", async ({ page }) => {
  await boot(page, { running: true, bpm: 60 }, { motion_allowed: false });
  const steps = [
    [0, "accent"],
    [1, "beat"],
  ] as const;
  for (const [i, kind] of steps) {
    await beat(page, { beat: i, kind, bpm: 60 });
    const lit = page.locator(`.td-dot[data-beat="${i}"] .td-dot-lit`);
    await expect(lit).toHaveCount(1);
    await page.waitForTimeout(700);
    expect(await lit.evaluate((el) => getComputedStyle(el).opacity)).toBe("1");
  }

  await setAppearance(page, { motion_allowed: true, animations: false });
  await beat(page, { beat: 2, kind: "beat", bpm: 60 });
  await page.waitForTimeout(500);
  expect(
    await page.locator(".td-dot-lit").evaluate((el) => getComputedStyle(el).opacity),
  ).toBe("1");
});

test("stopping clears the lit beat", async ({ page }) => {
  await boot(page, { running: true });
  await beat(page, { beat: 1, kind: "beat" });
  await expect(page.locator(".td-dot-lit")).toHaveCount(1);
  await setTransport(page, { running: false });
  await expect(page.locator(".td-dot-lit")).toHaveCount(0);
});

test("a meter change shows when the sound gets there, not before", async ({ page }) => {
  await boot(page, { running: true, beats_per_bar: 4 });
  await beat(page, { beat: 2, kind: "beat", beats_per_bar: 4 });
  // The setting changes mid bar; the bar that is playing keeps four beats.
  await setTransport(page, { beats_per_bar: 3 });
  await expect(page.locator(".td-dot")).toHaveCount(4);
  await beat(page, { beat: 0, kind: "accent", beats_per_bar: 3 });
  await expect(page.locator(".td-dot")).toHaveCount(3);
});

test("the bar sweep restarts on every downbeat", async ({ page }) => {
  await boot(page, { running: true, bpm: 240 }, { expanded: true });
  await page.setViewportSize({ width: 300, height: 88 });
  const group = page.locator(".td-group").nth(1);
  const clip = group.locator(".td-clip");
  const shift = () => clip.evaluate((el) => new DOMMatrix(getComputedStyle(el).transform).m41);

  await beat(page, { beat: 0, kind: "accent", bpm: 240 });
  await page.waitForTimeout(600);
  const late = await shift();
  await beat(page, { beat: 0, kind: "accent", bpm: 240, bar: 1 });
  await page.waitForTimeout(50);
  const early = await shift();
  // The clip is pulled left while the fill is short, so a fuller bar sits
  // nearer zero and a bar that has just restarted sits further left.
  expect(early).toBeLessThan(late);
  // The number is the bar the audio thread counted, not one the page made up.
  await expect(group.locator(".td-value").first()).toHaveText("2");
});
