// Smoke test of the app UI in demo mode (Vite dev server on :1440, installed Edge).
//   node apps/app/scripts/ui-check.mjs [--shot <file.png>] [--query "?theme=dark&lang=de"]
// Visits every page, fails on console errors or untranslated keys, optionally
// takes one screenshot of the overview.
import { chromium } from "playwright-core";

const args = process.argv.slice(2);
const shot = args.includes("--shot") ? args[args.indexOf("--shot") + 1] : null;
const query = args.includes("--query") ? args[args.indexOf("--query") + 1] : "?theme=dark";
const base = "http://localhost:1440/";

const browser = await chromium.launch({ channel: "msedge" });
const page = await browser.newPage({ viewport: { width: 1120, height: 780 }, deviceScaleFactor: 1 });
const errors = [];
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
page.on("pageerror", (e) => errors.push(e.message));

let failed = 0;
const check = (cond, name, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!cond) failed++;
};

await page.goto(base + query);
await page.waitForSelector("nav button");
const navItems = await page.locator("nav button").allTextContents();
check(navItems.length === 6, "six pages in the navigation", navItems.join(" | "));
const nav = (name) => page.locator("nav button").nth(["live", "profiles", "presets", "voices", "host", "settings"].indexOf(name)).click();

for (let i = 0; i < navItems.length; i++) {
  await page.locator("nav button").nth(i).click();
  await page.waitForTimeout(250);
  const text = await page.locator("main").innerText();
  // vue-i18n shows the key itself when a message is missing.
  const raw = text.match(/\b(common|nav|host|voices|live|profiles|settings|tips|app|presets|jobs|compute|editor|lang)\.[a-zA-Z.]+\b/g);
  check(!raw, `page "${navItems[i].trim()}" renders without raw i18n keys`, raw?.slice(0, 3).join(", ") ?? `${text.length} chars`);
}

// Tooltips: hovering an info button shows the explanation (settings page).
await nav("settings");
await page.locator(".tip-btn").first().hover();
await page.waitForTimeout(500);
check(await page.locator("#ttv-tip").isVisible(), "tooltip appears on hover");

// Voices: the library lists the demo voices and the test speaks.
await nav("voices");
await page.waitForTimeout(400);
const voiceCount = await page.locator("main ul li").count();
check(voiceCount >= 5, "voice library lists voices", String(voiceCount));
await page.locator("main").getByRole("button", { name: /^(speak|sprechen)$/i }).click();
await page.waitForTimeout(1600);
check(/1.2 s|1,2 s/.test(await page.locator("main").innerText()), "speech test shows its timing report");

// Voice host: live system tiles arrive.
await nav("host");
await page.waitForTimeout(600);
check((await page.locator("main").innerText()).includes("VoiceCreatorPro.exe"), "host page shows GPU processes of the VCP PC");

// Live: current message, queue and history with reasons; test message appears.
await nav("live");
await page.waitForTimeout(300);
const live = await page.locator("main").innerText();
check(live.includes("Mikey_TV") && /ignored user/.test(live), "live page shows the current message and filtered ones with a reason");
await page.locator("main textarea").fill("Hallo aus dem Test");
await page.locator("main").getByRole("button", { name: /^(send|senden)$/i }).click();
await page.waitForTimeout(300);
check((await page.locator("main").innerText()).includes("Hallo aus dem Test"), "test message shows up in the history");

// Profiles: editor shows the channel and saves changes.
await nav("profiles");
await page.waitForTimeout(300);
const prof = await page.locator("main").innerText();
check(prof.includes("zcarecrow"), "profile editor shows the channel");
check(/(Computed|Berechnet): de 41 · en 30/.test(prof), "profile shows how much of its preset is computed", prof.match(/(Computed|Berechnet):[^\n]*/)?.[0] ?? "");

// Presets: cards, 1.x adoption, details with listening, computing, own preset.
await nav("presets");
await page.waitForTimeout(500);
let pre = await page.locator("main").innerText();
check(/Cinematic/.test(pre) && /de 41/.test(pre) && /en 30/.test(pre), "preset card shows computed voices per language");
const adopt = page.locator("main").getByRole("button", { name: /adopt as preset|als preset übernehmen/i });
check(await adopt.isVisible(), "voices from TTV-TTS 1.x are offered for adoption");
await adopt.click();
await page.waitForTimeout(400);
check((await page.locator("main").innerText()).includes("Classic 30"), "adopted voices become the preset Classic 30");

await page.locator("article", { hasText: "Cinematic" }).getByRole("button", { name: /^(voices|stimmen)$/i }).click();
await page.waitForTimeout(400);
const dlg = page.locator("[role=dialog]");
check((await dlg.locator("ul li").count()) === 240, "details list all 240 voices", String(await dlg.locator("ul li").count()));
await dlg.getByPlaceholder(/filter/i).fill("baritone");
await page.waitForTimeout(150);
const filtered = await dlg.locator("ul li").count();
check(filtered > 5 && filtered < 120, "filter by sound narrows the list", String(filtered));
await dlg.getByPlaceholder(/filter/i).fill("");
await page.waitForTimeout(150);
await dlg.locator("ul li").first().getByRole("button").click();
await page.waitForTimeout(300);
check((await dlg.locator("ul li").first().locator(".spin").count()) === 1, "listening to a computed voice shows progress");
await page.waitForTimeout(1500);
await dlg.getByRole("button", { name: /^(compute|berechnen)$/i }).click();
await page.waitForTimeout(500);
const cd = page.locator("[role=dialog]");
const cdText = await cd.innerText();
check(/new voices|neue Stimmen/i.test(cdText) && /Takes about|Dauert etwa/.test(cdText), "compute dialog shows new voices and an estimate", cdText.match(/\d+ (new voices|neue Stimmen)/)?.[0] ?? "");
await cd.getByRole("button", { name: /^(start|starten)$/i }).click();
await page.waitForTimeout(1800);
pre = await page.locator("main").innerText();
const progress = pre.match(/(\d+) (of|von) (\d+) (voices|Stimmen)/);
check(/Computing|Berechnung/.test(pre) && progress && Number(progress[1]) > 0, "a started job shows progress", progress?.[0] ?? "");
await page.locator("main").getByRole("button", { name: /^(pause|pausieren)$/i }).first().click();
await page.waitForTimeout(300);
check(/paused|pausiert/.test(await page.locator("main").innerText()), "a job can be paused");

await page.locator("main").getByRole("button", { name: /new preset|neues preset/i }).click();
await page.waitForTimeout(300);
const ed = page.locator("[role=dialog]");
await ed.getByRole("button", { name: /type or paste|tippen oder einfügen/i }).click();
await ed.locator("textarea").fill("#ttv-tts preset v1\nname: Test Café\ndesc: Baristas\n---\nMia Bloom|f|27|Bright, quick mezzo with a laugh in every sentence.\nOtto Brenner|m|61|Low, grumbling bass, slow and dry, softhearted underneath.\nBroken|q|30|Some voice that has a bad gender field here.\n");
await page.waitForTimeout(600);
const edText = await ed.innerText();
check(/2 (of|von) \d+ (voices|Stimmen)/.test(edText) && /(Line|Zeile) 7/.test(edText), "editor counts valid voices and reports the bad line", edText.match(/(Line|Zeile) \d+:[^\n]*/)?.[0] ?? "");
await ed.getByRole("button", { name: /save 2 voices|2 stimmen speichern/i }).click();
await page.waitForTimeout(500);
check((await page.locator("main").innerText()).includes("Test Café"), "an own preset is created from pasted lines");
await page.keyboard.press("Escape");

if (shot) {
  await nav("live");
  await page.waitForTimeout(2300); // let the system stats arrive
  await page.screenshot({ path: shot });
  console.log(`screenshot: ${shot}`);
}

check(errors.length === 0, "no console errors", errors.slice(0, 3).join(" / "));
await browser.close();
process.exit(failed ? 1 : 0);
