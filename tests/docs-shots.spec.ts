import { test, type Browser } from "@playwright/test";

import {
  beat,
  boot,
  bootPicker,
  expectTransparentShot,
  opaqueSurface,
  requestedHeight,
  settle,
  type Json,
} from "./harness";

/**
 * Produces the images used in the README.
 *
 * npm run docs:shots
 *
 * Animation is off, so a lit dot is caught at full strength rather than at
 * whatever point of its fade the shutter fell on, and every regeneration is
 * byte for byte the last one unless the layout really changed.
 */

const OUT = "docs/media";
const DSF = 3;

/** Logical width of the floating window, from monitor.rs LOGICAL_W. */
const FLOAT_W = 300;

/** Corner radii from styles.css, in logical pixels. */
const FLOAT_RADIUS = 10;
const STRIP_RADIUS = 6;

/** The pinned strips, from monitor.rs, at a 48 pixel taskbar. */
const STRIP = { w: 208, h: 44 };
const COMPACT = { w: 124, h: 44 };

const STILL = { animations: false, motion_allowed: false } as const;

const PLAYING = {
  running: true,
  bpm: 96,
  beats_per_bar: 4,
  beat_unit: 4,
  subdivision: 2,
  active_device: "Speakers (USB Audio Interface)",
  output_device: "Speakers (USB Audio Interface)",
};

/**
 * The floating window is sized by the backend from the height the interface
 * asks for, so the image is too.
 */
async function floating(browser: Browser, file: string, appearance: Json, lit: Json) {
  const context = await browser.newContext({
    viewport: { width: FLOAT_W, height: 400 },
    deviceScaleFactor: DSF,
  });
  const page = await context.newPage();
  await boot(page, PLAYING, { ...STILL, ...appearance });
  await beat(page, { ...lit, bpm: PLAYING.bpm });
  await beat(page, { ...lit, bpm: PLAYING.bpm, bar: 11 });
  await settle(page);

  const height = await requestedHeight(page);
  test.expect(height, "the interface never reported a height").toBeGreaterThan(0);
  await page.setViewportSize({ width: FLOAT_W, height });

  await opaqueSurface(page);
  await settle(page);
  // omitBackground is the whole point. The page is already transparent, and
  // without it Chromium paints its own white base under the rounded corners.
  await page.screenshot({ path: `${OUT}/${file}`, omitBackground: true });
  await expectTransparentShot(page, `${OUT}/${file}`, {
    width: FLOAT_W * DSF,
    height: height * DSF,
    radius: FLOAT_RADIUS * DSF,
    light: appearance.resolved_theme === "light",
  });
  await context.close();
}

async function pinned(browser: Browser, file: string, size: { w: number; h: number }, appearance: Json) {
  const context = await browser.newContext({
    viewport: { width: size.w, height: size.h },
    deviceScaleFactor: DSF,
  });
  const page = await context.newPage();
  await boot(page, PLAYING, { ...STILL, placement: "taskbar", ...appearance });
  await beat(page, { beat: 0, kind: "accent", bpm: PLAYING.bpm });
  await opaqueSurface(page);
  await settle(page);
  await page.screenshot({ path: `${OUT}/${file}`, omitBackground: true });
  await expectTransparentShot(page, `${OUT}/${file}`, {
    width: size.w * DSF,
    height: size.h * DSF,
    radius: STRIP_RADIUS * DSF,
  });
  await context.close();
}

test("floating, dark", async ({ browser }) => {
  await floating(browser, "floating-dark.png", { resolved_theme: "dark" }, { beat: 2, kind: "beat" });
});

test("floating, light", async ({ browser }) => {
  await floating(browser, "floating-light.png", { resolved_theme: "light" }, { beat: 2, kind: "beat" });
});

test("floating, expanded", async ({ browser }) => {
  await floating(
    browser,
    "expanded-dark.png",
    { resolved_theme: "dark", expanded: true },
    { beat: 0, kind: "accent" },
  );
});

test("pinned to the taskbar", async ({ browser }) => {
  await pinned(browser, "taskbar.png", STRIP, { resolved_theme: "dark" });
});

test("pinned, compact", async ({ browser }) => {
  await pinned(browser, "taskbar-compact.png", COMPACT, { resolved_theme: "dark", compact: true });
});

test("the accent colour picker", async ({ browser }) => {
  const context = await browser.newContext({
    viewport: { width: 296, height: 372 },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();
  await bootPicker(page, { resolved_theme: "dark", accent: "#f28c38" });
  await test.expect(page.locator(".td-hex")).toHaveValue("#f28c38");
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  });
  await page.screenshot({ path: `${OUT}/accent-picker.png`, omitBackground: true });
  await context.close();
});
