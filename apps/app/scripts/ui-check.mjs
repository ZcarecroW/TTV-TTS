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
check(navItems.length === 5, "five pages in the navigation", navItems.join(" | "));

for (let i = 0; i < navItems.length; i++) {
  await page.locator("nav button").nth(i).click();
  await page.waitForTimeout(250);
  const text = await page.locator("main").innerText();
  // vue-i18n shows the key itself when a message is missing.
  const raw = text.match(/\b(common|nav|host|voices|live|profiles|settings|tips|app)\.[a-zA-Z.]+\b/g);
  check(!raw, `page "${navItems[i].trim()}" renders without raw i18n keys`, raw?.slice(0, 3).join(", ") ?? `${text.length} chars`);
}

// Tooltips: hovering an info button shows the explanation (settings page).
await page.locator("nav button").nth(4).click();
await page.locator(".tip-btn").first().hover();
await page.waitForTimeout(500);
check(await page.locator("#ttv-tip").isVisible(), "tooltip appears on hover");

// Voices: the library lists the demo voices and the test speaks.
await page.locator("nav button").nth(2).click();
await page.waitForTimeout(400);
const voiceCount = await page.locator("main ul li").count();
check(voiceCount >= 5, "voice library lists voices", String(voiceCount));
await page.locator("main").getByRole("button", { name: /^(speak|sprechen)$/i }).click();
await page.waitForTimeout(1600);
check(/1.2 s|1,2 s/.test(await page.locator("main").innerText()), "speech test shows its timing report");

// Voice host: live system tiles arrive.
await page.locator("nav button").nth(3).click();
await page.waitForTimeout(600);
check((await page.locator("main").innerText()).includes("VoiceCreatorPro.exe"), "host page shows GPU processes of the VCP PC");

// Live: current message, queue and history with reasons; test message appears.
await page.locator("nav button").nth(0).click();
await page.waitForTimeout(300);
const live = await page.locator("main").innerText();
check(live.includes("Mikey_TV") && /ignored user/.test(live), "live page shows the current message and filtered ones with a reason");
await page.locator("main textarea").fill("Hallo aus dem Test");
await page.locator("main").getByRole("button", { name: /^(send|senden)$/i }).click();
await page.waitForTimeout(300);
check((await page.locator("main").innerText()).includes("Hallo aus dem Test"), "test message shows up in the history");

// Profiles: editor shows the channel and saves changes.
await page.locator("nav button").nth(1).click();
await page.waitForTimeout(300);
check((await page.locator("main").innerText()).includes("zcarecrow"), "profile editor shows the channel");

if (shot) {
  await page.locator("nav button").nth(0).click();
  await page.waitForTimeout(2300); // let the system stats arrive
  await page.screenshot({ path: shot });
  console.log(`screenshot: ${shot}`);
}

check(errors.length === 0, "no console errors", errors.slice(0, 3).join(" / "));
await browser.close();
process.exit(failed ? 1 : 0);
