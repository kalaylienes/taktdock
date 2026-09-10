import { expect, test } from "@playwright/test";

import {
  DEFAULT_ACCENT,
  contrast,
  hexToHsv,
  hexToRgb,
  hsvToHex,
  normaliseHex,
  palette,
  pillBackground,
  type Theme,
} from "../src/lib/accent";
import { boot, bootPicker, calls, clearCalls, setAppearance } from "./harness";

/** Every hue at several lightnesses, plus the colours people try first. */
function sweep(): string[] {
  const out: string[] = ["#000000", "#ffffff", "#808080", "#ffff00", "#0000ff", "#00ff00", "#ff00ff"];
  for (let h = 0; h < 360; h += 15) {
    for (const [s, v] of [
      [1, 1],
      [1, 0.5],
      [0.4, 1],
      [0.8, 0.2],
      [0.1, 0.95],
    ]) {
      out.push(hsvToHex({ h, s, v }));
    }
  }
  return out;
}

test("pill text stays readable for any colour in either theme", () => {
  for (const theme of ["dark", "light"] as Theme[]) {
    for (const hex of sweep()) {
      const p = palette(hex, theme);
      const ratio = contrast(hexToRgb(p.ink), pillBackground(hex, theme));
      expect(ratio, `${hex} on ${theme}`).toBeGreaterThanOrEqual(4.5);
    }
  }
});

test("the chosen colour is used exactly as picked, and the rest follows it", () => {
  for (const theme of ["dark", "light"] as Theme[]) {
    for (const hex of sweep()) {
      const p = palette(hex, theme);
      expect(p.to).toBe(hex);
      expect(p.glow.slice(0, 7)).toBe(hex);
      // The dark end is never lighter than the colour, the core never darker.
      const lum = (c: string) => contrast(hexToRgb(c), [0, 0, 0]);
      expect(lum(p.from)).toBeLessThanOrEqual(lum(hex) + 0.01);
      expect(lum(p.core)).toBeGreaterThanOrEqual(lum(hex) - 0.01);
    }
  }
});

test("hex values are read however they are usually written", () => {
  expect(normaliseHex("#FF8800")).toBe("#ff8800");
  expect(normaliseHex("ff8800")).toBe("#ff8800");
  expect(normaliseHex(" #f80 ")).toBe("#ff8800");
  for (const bad of ["", "#", "#12", "#12345", "#ggg", "red"]) expect(normaliseHex(bad)).toBeNull();
});

test("the picker's colour space round trips", () => {
  for (const hex of sweep()) {
    if (hexToHsv(hex).s === 0) continue;
    expect(hsvToHex(hexToHsv(hex))).toBe(hex);
  }
});

const accentOf = (page: import("@playwright/test").Page) =>
  page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--takt-to").trim());

test("the widget wears a chosen accent and gives it back", async ({ page }) => {
  await boot(page, {}, { accent: "#f28c38" });
  await expect.poll(() => accentOf(page)).toBe("#f28c38");

  // The play button is drawn in it.
  const fill = await page.locator(".td-play svg").evaluate((el) => getComputedStyle(el).fill);
  expect(fill).toBe("rgb(242, 140, 56)");

  await setAppearance(page, { accent: null });
  await expect.poll(() => accentOf(page)).toBe(DEFAULT_ACCENT);
});

test("a colour being dragged in the picker shows at once and yields to the saved one", async ({
  page,
}) => {
  await boot(page);
  await page.evaluate(() => (window as any).__TD.emit("accent-preview", "#9b7ef0"));
  await expect.poll(() => accentOf(page)).toBe("#9b7ef0");
  await setAppearance(page, { accent: "#58a6ff" });
  await expect.poll(() => accentOf(page)).toBe("#58a6ff");
});

test("the light theme gets its own ink for the same colour", async ({ page }) => {
  await boot(page, {}, { accent: "#f5c84e" });
  const ink = () =>
    page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--takt-ink").trim());
  const dark = await ink();
  await setAppearance(page, { resolved_theme: "light" });
  await expect.poll(ink).not.toBe(dark);
});

test("dragging in the picker previews, and letting go saves", async ({ page }) => {
  await bootPicker(page);
  await page.setViewportSize({ width: 296, height: 372 });
  await clearCalls(page);

  const sv = (await page.locator(".td-sv").boundingBox())!;
  await page.mouse.move(sv.x + sv.width * 0.2, sv.y + sv.height * 0.2);
  await page.mouse.down();
  await page.mouse.move(sv.x + sv.width * 0.9, sv.y + sv.height * 0.1, { steps: 6 });
  await page.mouse.up();

  await expect.poll(async () => (await calls(page, "preview_accent")).length).toBeGreaterThan(0);
  const saved = await calls(page, "set_accent");
  expect(saved).toHaveLength(1);
  expect(saved[0].args.color).toMatch(/^#[0-9a-f]{6}$/);
  await expect(page.locator(".td-hex")).toHaveValue(saved[0].args.color);
});

test("the hue strip moves around the wheel", async ({ page }) => {
  await bootPicker(page, { accent: "#ff0000" });
  await page.setViewportSize({ width: 296, height: 372 });
  await expect(page.locator(".td-hex")).toHaveValue("#ff0000");
  await clearCalls(page);

  const hue = (await page.locator(".td-hue").boundingBox())!;
  await page.mouse.click(hue.x + hue.width / 3, hue.y + hue.height / 2);
  const saved = (await calls(page, "set_accent")).map((c) => c.args.color);
  // A third of the way round from red is green.
  expect(saved[saved.length - 1]).toBe("#00ff00");
});

test("a typed hex value is saved, and nonsense is refused", async ({ page }) => {
  await bootPicker(page);
  await clearCalls(page);
  const field = page.locator(".td-hex");

  await field.fill("#12ab9");
  await field.press("Enter");
  await expect(field).toHaveClass(/td-hex-invalid/);
  expect(await calls(page, "set_accent")).toHaveLength(0);

  await field.fill("C0FFEE");
  await field.press("Enter");
  await expect(field).toHaveValue("#c0ffee");
  expect((await calls(page, "set_accent")).map((c) => c.args.color)).toEqual(["#c0ffee"]);
});

test("a preset saves at once, Default goes back to the built in colour", async ({ page }) => {
  await bootPicker(page);
  await clearCalls(page);
  await page.locator('.td-preset[title="#f06fa8"]').click();
  await page.locator(".td-picker-link", { hasText: "Default" }).click();
  expect((await calls(page, "set_accent")).map((c) => c.args.color)).toEqual(["#f06fa8", null]);
  await expect(page.locator(".td-picker-link")).toBeDisabled();
});

test("cancel puts back the colour the window opened with and closes it", async ({ page }) => {
  await bootPicker(page, { accent: "#58a6ff" });
  await expect(page.locator(".td-hex")).toHaveValue("#58a6ff");
  await page.locator('.td-preset[title="#ef5350"]').click();
  await clearCalls(page);

  await page.locator(".td-button", { hasText: "Cancel" }).click();
  expect((await calls(page, "set_accent")).map((c) => c.args.color)).toEqual(["#58a6ff"]);
  expect(await calls(page, "plugin:window|close")).toHaveLength(1);
});

test("escape cancels too", async ({ page }) => {
  await bootPicker(page, { accent: null });
  await page.locator('.td-preset[title="#7cc86d"]').click();
  await clearCalls(page);
  await page.keyboard.press("Escape");
  expect((await calls(page, "set_accent")).map((c) => c.args.color)).toEqual([null]);
});
