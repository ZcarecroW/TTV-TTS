// Takes the screenshots for the documentation and the README from the demo
// mode of both apps (a mocked backend: nothing touches Twitch or Voice
// Creator Pro), in English and German:
//   node docs/site/shots.mjs                 all shots, both languages
//   node docs/site/shots.mjs --lang de       only German
//   node docs/site/shots.mjs live presets    only these
// Needs Microsoft Edge (or TTV_BROWSER=chrome). Writes docs/screenshots/*.webp
// (English) and docs/screenshots/de/*.webp (German).
import { createServer } from "vite";
import { chromium } from "playwright-core";
import { mkdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const W = 1280, H = 800;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const en = (await import("../../apps/app/src/i18n/en.ts")).default;
const de = (await import("../../apps/app/src/i18n/de.ts")).default;
const exact = (s) => new RegExp(`^${s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`);

// Visible texts the shots click on, per interface language.
const L = Object.fromEntries(
  [["en", en], ["de", de]].map(([k, m]) => [k, { details: exact(m.presets.details), compute: exact(m.presets.compute), newPreset: exact(m.presets.new), pair: /pair a device|gerät koppeln/i, pluginsAdd: exact(m.settings.pluginsAdd) }]),
);

const APP = ["live", "profiles", "viewers", "presets", "voices", "host", "settings"];
const PROXY = ["overview", "connect", "vcp", "obs", "internet", "firewall", "log", "settings"];
const nav = (i) => async (p) => {
  await p.locator("nav button").nth(i).click();
  await wait(500);
};
const click = (re, scope = "main") => async (p) => {
  await p.locator(scope).getByRole("button", { name: re }).first().click();
  await wait(600);
};
/** Crop to one element (scrolled into view) plus padding. A card in a grid
 * row is as tall as the row: its height then ends below its content. */
const card = (selector, pad = 16) => async (p) => {
  const el = p.locator(selector).first();
  await el.evaluate((e) => e.scrollIntoView({ block: "center" }));
  await wait(350);
  const b = await el.evaluate((e) => {
    const r = e.getBoundingClientRect();
    const bottom = Math.max(...[...e.children].map((c) => c.getBoundingClientRect().bottom));
    const inner = parseFloat(getComputedStyle(e).paddingBottom) || 0;
    return { x: r.x, y: r.y, width: r.width, height: Math.min(r.height, bottom - r.y + inner) };
  });
  const x = Math.max(0, b.x - pad);
  const y = Math.max(0, b.y - pad);
  return { x, y, width: Math.min(W - x, b.width + 2 * pad), height: Math.min(H - y, b.height + 2 * pad) };
};
const dialog = () => card("[role=dialog]", 2);
/** Where the voices play: 0 sound device, 1 OBS on this PC, 2 through the voice host PC. */
const output = (i) => async (p) => {
  await p.locator("[data-output] .inline-flex button").nth(i).click();
  await wait(2600); // saved after 600 ms, then the card looks again
};

// name → app, query, steps, optional crop
const shots = (t) => ({
  live: { app: "app", steps: [nav(0)] },
  "live-light": { app: "app", q: "theme=light", steps: [nav(0)] },
  "live-approval": { app: "app", q: "approval", steps: [nav(0)] },
  profiles: { app: "app", steps: [nav(1)] },
  "profiles-events": { app: "app", steps: [nav(1)], crop: card("main [data-events]") },
  "profiles-translate": { app: "app", steps: [nav(1)], crop: card("main [data-translate]") },
  "profile-output": { app: "app", q: "obs", steps: [nav(1), output(1)], crop: card("main [data-output]") },
  "profile-output-host": { app: "app", q: "obs", steps: [nav(1), output(2)], crop: card("main [data-output]") },
  "live-hosted": { app: "app", q: "obs", steps: [nav(1), output(2), nav(0)], crop: card("main [data-hosted]") },
  viewers: { app: "app", steps: [nav(2)] },
  presets: { app: "app", steps: [nav(3)] },
  "preset-detail": { app: "app", steps: [nav(3), click(t.details)] },
  "preset-compute": { app: "app", steps: [nav(3), click(t.compute)] },
  "preset-editor": { app: "app", steps: [nav(3), click(t.newPreset)] },
  voices: { app: "app", steps: [nav(4)] },
  host: { app: "app", steps: [nav(5)] },
  "host-setup": { app: "app", q: "nohost", steps: [nav(5)] },
  settings: { app: "app", steps: [nav(6)] },
  "settings-plugins": { app: "app", steps: [nav(6)], crop: card("main [data-plugins]") },
  "settings-viewer-plugins": { app: "app", steps: [nav(6)], crop: card("main [data-viewer-variants]") },
  "settings-os-voices": { app: "app", steps: [nav(6)], crop: card("main [data-os-voices]") },
  "settings-twitch": { app: "app", steps: [nav(6)], crop: card("main [data-twitch]") },
  "settings-remote": { app: "app", steps: [nav(6)], crop: card("main [data-remote]") },
  "settings-hotkeys": { app: "app", steps: [nav(6)], crop: card("main [data-hotkeys]") },
  "proxy-overview": { app: "proxy", steps: [nav(0)] },
  "proxy-connect": { app: "proxy", steps: [nav(1)] },
  "proxy-pairing": { app: "proxy", steps: [nav(0), click(t.pair)], crop: dialog() },
  "proxy-vcp": { app: "proxy", steps: [nav(2)] },
  "proxy-obs": { app: "proxy", q: "obs", steps: [nav(3)] },
  "proxy-internet": { app: "proxy", steps: [nav(4)] },
  "proxy-internet-https": { app: "proxy", q: "https", steps: [nav(4)], crop: card("main [data-duckdns]") },
  "proxy-firewall": { app: "proxy", steps: [nav(5)] },
  "proxy-settings": { app: "proxy", steps: [nav(7)] },
  "proxy-setup": { app: "proxy", q: "setup" },
});
if (APP.length !== 7 || PROXY.length !== 8) throw new Error("navigation changed");

const args = process.argv.slice(2);
const langArg = args.indexOf("--lang");
const langs = langArg >= 0 ? [args[langArg + 1]] : ["en", "de"];
const only = langArg >= 0 ? args.filter((_, i) => i !== langArg && i !== langArg + 1) : args;

const serve = async (dir, port) => {
  const s = await createServer({ root: join(root, dir), configFile: join(root, dir, "vite.config.ts"), logLevel: "error", server: { port, strictPort: false } });
  await s.listen();
  return s;
};
const servers = { app: await serve("apps/app", 1460), proxy: await serve("apps/proxy", 1461) };
const browser = await chromium.launch({ channel: process.env.TTV_BROWSER ?? "msedge", headless: true });
let failed = 0;
try {
  for (const lang of langs) {
    const out = join(root, "docs/screenshots", lang === "en" ? "" : lang);
    mkdirSync(out, { recursive: true });
    for (const [name, s] of Object.entries(shots(L[lang]))) {
      if (only.length && !only.includes(name)) continue;
      const q = s.q ?? "";
      const theme = q.includes("theme=light") ? "light" : "dark";
      const page = await browser.newPage({ viewport: { width: W, height: H }, colorScheme: theme, locale: lang === "de" ? "de-DE" : "en-US" });
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      try {
        const base = servers[s.app].resolvedUrls.local[0];
        await page.goto(`${base}?lang=${lang}&${q.includes("theme=") ? q : `${q}&theme=dark`}`);
        await page.waitForSelector(s.q === "setup" ? "[role=dialog], main" : "nav button");
        await wait(1000); // demo data, fonts and transitions settle
        for (const step of s.steps ?? []) await step(page);
        await page.mouse.move(W - 2, H - 2);
        await wait(250);
        const file = join(out, `${name}.webp`);
        const clip = s.crop ? await s.crop(page) : undefined;
        await page.screenshot({ path: file, type: "webp", quality: 82, clip });
        console.log(`${lang}/${name}.webp ${(statSync(file).size / 1024).toFixed(0)} KB${errors.length ? ` – page errors: ${errors.join("; ")}` : ""}`);
      } catch (e) {
        failed++;
        console.error(`${lang}/${name}: ${e.message.split("\n")[0]}`);
      } finally {
        await page.close();
      }
    }
  }
} finally {
  await browser.close();
  await servers.app.close();
  await servers.proxy.close();
}
process.exit(failed ? 1 : 0);
