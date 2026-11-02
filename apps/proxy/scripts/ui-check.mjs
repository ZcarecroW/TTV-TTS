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
check(navItems.length === 8, "eight pages in the navigation", navItems.join(" | "));

for (let i = 0; i < navItems.length; i++) {
  await page.locator("nav button").nth(i).click();
  await page.waitForTimeout(250);
  const text = await page.locator("main").innerText();
  // vue-i18n shows the key itself when a message is missing.
  const raw = text.match(/\b(common|nav|status|overview|connect|vcp|firewall|log|settings|setup|tips|friends|obs)\.[a-zA-Z.]+\b/g);
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

// VCP page: speed and the restart-when-slow switch
await page.locator("nav button").nth(navItems.findIndex((n) => /VCP|Voice Creator/i.test(n))).click();
await page.waitForTimeout(300);
const vcpText = await page.locator("main").innerText();
check(/0\.55 s (per second of speech|pro Sekunde Sprache)/.test(vcpText) && /(usually|sonst) 0\.50/.test(vcpText), "the VCP page shows its speed", vcpText.match(/0\.55[^\n]*/)?.[0] ?? "");
check(/Restart VCP when it gets slow|VCP neu starten, wenn es langsam wird/.test(vcpText), "VCP can be restarted when it gets slow");
// Internet page: the same friends card as in the app, run from the proxy window
await page.locator("nav button").nth(navItems.findIndex((n) => /Internet/i.test(n))).click();
await page.waitForTimeout(300);
{
  const card = page.locator("main section").first();
  check(/Friends over the internet|Freunde über das Internet/.test(await card.innerText()) && /Mia/.test(await card.innerText()), "the internet page shows friends with access");
  await card.getByRole("switch").first().click();
  await page.waitForTimeout(300);
  check(/8111/.test(await card.locator(".badge").first().innerText()), "internet access can be switched on in the proxy window", await card.locator(".badge").first().innerText().catch(() => ""));
  await card.locator("input").first().fill("my-stream");
  await card.locator("input[type=password]").fill("1a2b3c4d-1a2b-3c4d-5e6f-1a2b3c4d5e6f");
  await card.getByRole("button", { name: /^(Save|Speichern)$/ }).click();
  await page.waitForTimeout(300);
  check(/my-stream\.duckdns\.org/.test(await page.locator("body").innerText()), "a DuckDNS name can be set");
  await card.getByRole("button", { name: /3 ·/ }).click();
  await page.waitForTimeout(800);
  check(/(reachable|erreichbar)/i.test(await card.innerText()), "the reachability test runs");
  await card.locator("input").nth(2).fill("Alex");
  await card.getByRole("button", { name: /^(Create code|Code erstellen)$/ }).click();
  await page.waitForTimeout(300);
  check(/ttv1:/.test(await card.locator("code").first().innerText().catch(() => "")) && /Alex/.test(await card.innerText()), "an invite code is made for a friend");
}
check(errors.length === 0, "no console errors", errors.slice(0, 3).join(" / "));
await browser.close();
process.exit(failed ? 1 : 0);
