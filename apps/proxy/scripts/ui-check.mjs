// Smoke test of the proxy UI in demo mode (Vite dev server on :1430, installed Edge).
//   node apps/proxy/scripts/ui-check.mjs [--shot <file.png>] [--query "?theme=dark&lang=de"]
// Visits every page, fails on console errors or untranslated keys, optionally
// takes one screenshot of the overview.
import { chromium } from "playwright-core";

const args = process.argv.slice(2);
const shot = args.includes("--shot") ? args[args.indexOf("--shot") + 1] : null;
const query = args.includes("--query") ? args[args.indexOf("--query") + 1] : "?theme=dark";
const base = "http://localhost:1430/";

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

for (let i = 0; i < navItems.length; i++) {
  await page.locator("nav button").nth(i).click();
  await page.waitForTimeout(250);
  const text = await page.locator("main").innerText();
  // vue-i18n shows the key itself when a message is missing.
  const raw = text.match(/\b(common|nav|status|overview|connect|vcp|firewall|log|settings|setup|tips)\.[a-zA-Z.]+\b/g);
  check(!raw, `page "${navItems[i].trim()}" renders without raw i18n keys`, raw?.slice(0, 3).join(", ") ?? `${text.length} chars`);
}

// Tooltips: hovering an info button shows the explanation.
await page.locator("nav button").nth(1).click();
await page.locator(".tip-btn").first().hover();
await page.waitForTimeout(500);
check(await page.locator("#ttv-tip").isVisible(), "tooltip appears on hover");

// Pairing dialog opens with a 6-digit code.
await page.locator("nav button").nth(0).click();
await page.locator("main").getByRole("button", { name: /pair a device|gerät koppeln/i }).first().click();
await page.waitForSelector("[role=dialog]");
const digits = await page.locator("[role=dialog] .font-mono.text-3xl").allTextContents();
check(digits.join("").length === 6, "pairing dialog shows a 6-digit code", digits.join(""));
await page.keyboard.press("Escape");

if (shot) {
  await page.locator("nav button").nth(0).click();
  await page.waitForTimeout(2300); // let the system stats arrive
  await page.screenshot({ path: shot });
  console.log(`screenshot: ${shot}`);
}

check(errors.length === 0, "no console errors", errors.slice(0, 3).join(" / "));
await browser.close();
process.exit(failed ? 1 : 0);
