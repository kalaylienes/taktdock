import { test, type Browser } from "@playwright/test";

import {
  beat,
  boot,
  bootPicker,
  expectTransparentShot,
  installTauriMock,
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

/**
 * Scenes: the real widget, running in frames, placed on a drawn desktop.
 *
 * The desktop is an illustration, deliberately generic: a wallpaper, one
 * window, and a taskbar with plain tiles where the app icons would be. No
 * real screen is photographed, because a real screen shows somebody's apps.
 * The widgets in it are the real interface, driven through the same fake
 * backend as the tests; each frame reads its settings from the URL fragment.
 */

interface Widget {
  /** Where the frame sits in the scene, in CSS pixels. */
  style: string;
  transport?: Json;
  appearance?: Json;
  /** The beat to light once it has loaded. */
  lit?: Json;
}

const TIME = `<div class="clock"><span>16:30</span><span>11/09/2026</span></div>`;

function desktop(width: number, height: number, widgets: Widget[], transparent = false): string {
  const frames = widgets
    .map((w) => {
      const cfg = encodeURIComponent(
        JSON.stringify({
          transport: { ...PLAYING, ...(w.transport ?? {}) },
          appearance: { ...STILL, resolved_theme: "dark", ...(w.appearance ?? {}) },
        }),
      );
      return `<iframe style="${w.style}" src="http://localhost:1420/#${cfg}"></iframe>`;
    })
    .join("");
  const scenery = transparent
    ? ""
    : `
      <div class="window">
        <div class="title"><span></span><span></span><span></span>practice-notes.txt</div>
        <div class="body">
          <p><b>Monday</b></p>
          <p>Chromatic warm up, 4/4 in sixteenths, 80 to 104 bpm</p>
          <p>Alternate picking, 6/8, start at 90</p>
          <p>Song: verse riff at 112, chorus with the click on 2 and 4</p>
          <p>Tap the tempo off the record before learning the solo</p>
        </div>
      </div>
      <div class="taskbar">
        <div class="apps">${'<i></i>'.repeat(7)}</div>
        <div class="tray"><em>^</em><u></u><u></u><u></u>${TIME}</div>
      </div>`;
  return `<!doctype html><html><head><style>
    html, body { margin: 0; width: ${width}px; height: ${height}px; overflow: hidden;
      font-family: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
      background: ${
        transparent
          ? "transparent"
          : "radial-gradient(900px 500px at 15% 0%, #2b4c6f 0%, transparent 70%), radial-gradient(800px 500px at 95% 20%, #4a2a5e 0%, transparent 65%), linear-gradient(160deg, #101826, #1a1024)"
      }; }
    iframe { position: absolute; border: 0; background: transparent; }
    .window { position: absolute; left: 48px; top: 36px; width: 460px; height: ${height - 130}px;
      border-radius: 8px; background: #202020; box-shadow: 0 12px 40px rgba(0,0,0,.45), 0 0 0 1px rgba(255,255,255,.06); overflow: hidden; }
    .window .title { height: 32px; display: flex; align-items: center; gap: 6px; padding: 0 12px;
      color: #cfcfcf; font-size: 12px; background: #2a2a2a; }
    .window .title span { width: 10px; height: 10px; border-radius: 50%; background: #444; }
    .window .title span:last-of-type { margin-right: 6px; }
    .window .body { padding: 10px 16px; color: #d8d8d8; font-size: 13px; line-height: 1.5; }
    .window .body p { margin: 0 0 4px; }
    .taskbar { position: absolute; left: 0; right: 0; bottom: 0; height: 48px;
      background: rgba(28,28,30,.94); border-top: 1px solid rgba(255,255,255,.07);
      display: flex; align-items: center; }
    .apps { position: absolute; left: 50%; transform: translateX(-50%); display: flex; gap: 8px; }
    .apps i { width: 32px; height: 32px; border-radius: 6px; background: linear-gradient(145deg, #3b3f47, #2b2e34);
      box-shadow: inset 0 0 0 1px rgba(255,255,255,.06); }
    .apps i:first-child { background: linear-gradient(145deg, #4e7bd8, #2f5bb7); }
    .tray { position: absolute; right: 12px; top: 0; bottom: 0; display: flex; align-items: center; gap: 10px;
      color: #e6e6e6; font-size: 12px; }
    .tray em { font-style: normal; opacity: .8; }
    .tray u { width: 14px; height: 14px; border-radius: 3px; background: rgba(255,255,255,.55); text-decoration: none; }
    .clock { display: flex; flex-direction: column; align-items: flex-end; line-height: 1.25; }
  </style></head><body>${scenery}${frames}</body></html>`;
}

async function scene(
  browser: Browser,
  file: string,
  size: { w: number; h: number },
  widgets: Widget[],
  transparent = false,
) {
  const context = await browser.newContext({
    viewport: { width: size.w, height: size.h },
    deviceScaleFactor: 2,
  });
  const page = await context.newPage();
  await installTauriMock(page);
  // Runs in every frame as well as the page, and each widget frame takes its
  // settings from its own address.
  await page.addInitScript(() => {
    const raw = decodeURIComponent(location.hash.slice(1));
    if (!raw) return;
    const cfg = JSON.parse(raw);
    const apply = () => {
      const td = (window as any).__TD;
      if (!td) return false;
      td.transport = { ...td.transport, ...cfg.transport };
      td.appearance = { ...td.appearance, ...cfg.appearance };
      return true;
    };
    if (!apply()) queueMicrotask(apply);
  });
  await page.setContent(desktop(size.w, size.h, widgets, transparent));

  const frames = page.frames().filter((f) => f !== page.mainFrame());
  test.expect(frames).toHaveLength(widgets.length);
  for (const [i, frame] of frames.entries()) {
    await frame.waitForSelector(".td-dot");
    await frame.addStyleTag({
      content: `html[data-theme="dark"], :root { --surface: #1b1b1bf2; } html[data-theme="light"] { --surface: #f7f7f5f2; }`,
    });
    const lit = widgets[i].lit;
    if (lit) {
      await frame.evaluate((l) => {
        const td = (window as any).__TD;
        td.emit("beat", {
          bar: 3,
          sub: 0,
          beats_per_bar: td.transport.beats_per_bar,
          subdivision: td.transport.subdivision,
          bpm: td.transport.bpm,
          at_ms: Date.now(),
          ...l,
        });
      }, lit);
    }
    await frame.evaluate(async () => {
      await document.fonts.ready;
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    });
  }
  await page.waitForTimeout(150);
  await page.screenshot({ path: `${OUT}/${file}`, omitBackground: transparent });
  await context.close();
}

const SCENE = { w: 1100, h: 330 };

test("scene: floating above the taskbar", async ({ browser }) => {
  await scene(browser, "scene-floating.png", SCENE, [
    {
      // Twelve in from the right and eight above the taskbar, as monitor.rs
      // places it.
      style: `right: 12px; bottom: ${48 + 8}px; width: 300px; height: 50px;`,
      lit: { beat: 1, kind: "beat" },
    },
  ]);
});

test("scene: pinned into the taskbar", async ({ browser }) => {
  // Left of the notification area by the default gap, two pixels in from the
  // top and bottom of the strip.
  await scene(browser, "scene-pinned.png", SCENE, [
    {
      style: "right: 196px; bottom: 2px; width: 208px; height: 44px;",
      appearance: { placement: "taskbar" },
      lit: { beat: 0, kind: "accent" },
    },
  ]);
});

test("scene: any accent colour", async ({ browser }) => {
  const accents: Array<[string | null, string]> = [
    [null, "dark"],
    ["#f28c38", "dark"],
    ["#58a6ff", "light"],
    ["#f06fa8", "dark"],
    ["#7cc86d", "light"],
  ];
  await scene(
    browser,
    "accents.png",
    { w: 300, h: accents.length * 62 - 12 },
    accents.map(([accent, theme], i) => ({
      style: `left: 0; top: ${i * 62}px; width: 300px; height: 50px;`,
      appearance: { accent, resolved_theme: theme },
      transport: { bpm: 72 + i * 24, beats_per_bar: [4, 3, 4, 6, 7][i], beat_unit: [4, 4, 4, 8, 8][i] },
      lit: { beat: i % 3, kind: i % 3 === 0 ? "accent" : "beat" },
    })),
    true,
  );
});
