import { expect, test } from "@playwright/test";

import { beat, boot, setAppearance } from "./harness";

/**
 * Pixel level checks that catch what a functional assertion misses: glow
 * bleeding past a six pixel bar, text that no longer fits its slot, a theme
 * that loses contrast, or a layout that grows a scrollbar.
 */

test("the glow stays close to the bar and the dots", async ({ browser }) => {
  const context = await browser.newContext({
    viewport: { width: 300, height: 50 },
    deviceScaleFactor: 4,
  });
  const page = await context.newPage();
  await boot(page, { running: true, bpm: 200 });
  await beat(page, { beat: 1, kind: "beat" });
  await page.waitForTimeout(600);

  const spreads = await page.evaluate(() =>
    [".td-glow", ".td-dot-lit"].map((sel) => {
      const el = document.querySelector(sel) as HTMLElement;
      const shadow = getComputedStyle(el).boxShadow;
      // The outer shadow is the only one that is not inset.
      const outer = shadow.split(/,(?![^(]*\))/).find((s) => !s.includes("inset")) ?? "";
      const lengths = (outer.match(/(-?\d+(?:\.\d+)?)px/g) ?? []).map((v) => parseFloat(v));
      return { sel, blur: lengths[2] ?? 0, spread: lengths[3] ?? 0 };
    }),
  );
  // A six pixel bar cannot carry more glow than its own height without the
  // bleed reading as a halo above and below the row.
  for (const s of spreads) expect(s.blur + s.spread, s.sel).toBeLessThanOrEqual(4);
  await context.close();
});

test("the widest values fit their slots without clipping", async ({ page }) => {
  const cases = [
    { transport: { bpm: 300, beats_per_bar: 7, beat_unit: 8, subdivision: 3 }, appearance: {} },
    { transport: { bpm: 300, subdivision: 4, beat_unit: 8, beats_per_bar: 6 }, appearance: {} },
    {
      transport: {
        bpm: 188,
        volume: 100,
        sound: "wood",
        output_device: null,
        active_device: null,
      },
      appearance: { expanded: true },
    },
    { transport: { bpm: 300, beats_per_bar: 7, beat_unit: 8 }, appearance: { placement: "taskbar" } },
    {
      transport: { bpm: 300, beats_per_bar: 7, beat_unit: 8 },
      appearance: { placement: "taskbar", compact: true },
    },
  ];
  const sizes: Record<string, [number, number]> = {
    float: [300, 50],
    expanded: [300, 88],
    taskbar: [208, 44],
    compact: [124, 44],
  };

  for (const c of cases) {
    await boot(page, c.transport, c.appearance);
    const a = c.appearance as Record<string, unknown>;
    const key = a.compact ? "compact" : a.placement ? "taskbar" : a.expanded ? "expanded" : "float";
    const [w, h] = sizes[key];
    await page.setViewportSize({ width: w, height: h });
    await page.waitForTimeout(200);

    const clipped = await page.evaluate(() =>
      Array.from(document.querySelectorAll(".td-value, .td-tail, .td-label, .td-pill, .td-bpm"))
        .map((el) => ({
          text: (el.textContent ?? "").trim(),
          overflowing: el.scrollWidth > el.clientWidth + 1,
        }))
        .filter((c) => c.overflowing),
    );
    expect(clipped, `${key}: ${JSON.stringify(clipped)}`).toHaveLength(0);
  }
});

test("a long device name is cut with an ellipsis, not pushed out of the row", async ({ page }) => {
  await boot(
    page,
    {
      output_device: "Speakers (Focusrite Scarlett 2i2 4th Gen USB Audio Interface)",
      active_device: "Speakers (Focusrite Scarlett 2i2 4th Gen USB Audio Interface)",
    },
    { expanded: true },
  );
  await page.setViewportSize({ width: 300, height: 88 });
  const device = page.locator(".td-device");
  const box = (await device.boundingBox())!;
  expect(box.x + box.width).toBeLessThanOrEqual(193 + 1);
  expect(await device.evaluate((el) => getComputedStyle(el).textOverflow)).toBe("ellipsis");
});

test("a device problem is shown in the warning colour with the reason as its tooltip", async ({
  page,
}) => {
  const problem = "Focusrite USB not found, using the default output";
  await boot(
    page,
    { output_device: "Focusrite USB", active_device: "Speakers", device_problem: problem },
    { expanded: true },
  );
  await page.setViewportSize({ width: 300, height: 88 });
  const device = page.locator(".td-device");
  await expect(device).toHaveClass(/td-warn/);
  await expect(device).toHaveAttribute("title", problem);
  await expect(device).toHaveText("Speakers");
});

test("both themes keep the tempo and the pills legible", async ({ page }) => {
  await boot(page);

  for (const theme of ["dark", "light"] as const) {
    await setAppearance(page, { resolved_theme: theme });
    await page.waitForTimeout(200);

    const contrast = await page.evaluate(() => {
      // Colours are painted and read back as pixels, because computed styles
      // come back in whatever space the browser prefers, mixes included.
      const canvas = document.createElement("canvas");
      canvas.width = 1;
      canvas.height = 1;
      const ctx = canvas.getContext("2d")!;
      const rgb = (colour: string, over = "#000") => {
        ctx.clearRect(0, 0, 1, 1);
        ctx.fillStyle = over;
        ctx.fillRect(0, 0, 1, 1);
        ctx.fillStyle = colour;
        ctx.fillRect(0, 0, 1, 1);
        const d = ctx.getImageData(0, 0, 1, 1).data;
        return [d[0], d[1], d[2]] as [number, number, number];
      };
      const lum = ([r, g, b]: [number, number, number]) => {
        const f = (v: number) => {
          const s = v / 255;
          return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
        };
        return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
      };
      const ratio = (a: number, b: number) => {
        const [hi, lo] = a > b ? [a, b] : [b, a];
        return (hi + 0.05) / (lo + 0.05);
      };
      const light = document.documentElement.dataset.theme === "light";
      const backdrop = light ? "#f7f7f5" : "#1b1b1b";
      const surface = lum(rgb(getComputedStyle(document.querySelector(".td-rows")!).backgroundColor, backdrop));
      const bpm = lum(rgb(getComputedStyle(document.querySelector(".td-bpm")!).color, backdrop));
      const pill = document.querySelector(".td-pill") as HTMLElement;
      const pillBg = rgb(getComputedStyle(pill).backgroundColor, backdrop);
      const pillText = lum(rgb(getComputedStyle(pill).color, backdrop));
      return { bpm: ratio(bpm, surface), pill: ratio(pillText, lum(pillBg)) };
    });

    expect(contrast.bpm, `${theme} tempo contrast`).toBeGreaterThan(4.5);
    expect(contrast.pill, `${theme} pill contrast`).toBeGreaterThan(4.5);
  }
});

test("the widget never scrolls in any supported layout", async ({ page }) => {
  const cases: Array<{ appearance: Record<string, unknown>; w: number; h: number }> = [
    { appearance: {}, w: 300, h: 50 },
    { appearance: { expanded: true }, w: 300, h: 88 },
    { appearance: { placement: "taskbar" }, w: 208, h: 44 },
    { appearance: { placement: "taskbar" }, w: 208, h: 32 },
    { appearance: { placement: "taskbar", compact: true }, w: 124, h: 44 },
    { appearance: { placement: "taskbar", compact: true }, w: 124, h: 32 },
  ];

  for (const c of cases) {
    await boot(page, { bpm: 300, beats_per_bar: 7, beat_unit: 8 }, c.appearance);
    await page.setViewportSize({ width: c.w, height: c.h });
    await page.waitForTimeout(200);

    const box = await page.evaluate(() => ({
      sh: document.documentElement.scrollHeight,
      ih: window.innerHeight,
      sw: document.documentElement.scrollWidth,
      iw: window.innerWidth,
    }));
    expect(box.sh, JSON.stringify(c)).toBeLessThanOrEqual(box.ih);
    expect(box.sw, JSON.stringify(c)).toBeLessThanOrEqual(box.iw);
  }
});
