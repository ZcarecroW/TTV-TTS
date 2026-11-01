// Checks the REAL app (real backend, proxy and VCP) by driving its window
// over WebView2 remote debugging. Start the app with
//   cd apps/app && npx tauri dev --config src-tauri/tauri.e2e.conf.json
// then: node apps/app/scripts/real-check.mjs [--speak]
// Read-only except --speak (one test message is read aloud on the real output).
import { chromium } from "playwright-core";

const speak = process.argv.includes("--speak");
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const page = browser.contexts().flatMap((c) => c.pages()).find((p) => /localhost:1440|tauri\.localhost/.test(p.url()));
if (!page) throw new Error("app window not found");
const errors = [];
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
page.on("pageerror", (e) => errors.push(e.message));
let failed = 0;
const check = (cond, name, extra = "") => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!cond) failed++;
};
const pages = ["live", "profiles", "viewers", "presets", "voices", "host", "settings"];
const nav = async (name) => {
  await page.locator("nav button").nth(pages.indexOf(name)).click();
  await page.waitForTimeout(600);
};
const main = () => page.locator("main").innerText();

await page.reload();
await page.waitForSelector("nav button", { timeout: 20000 });
await page.waitForTimeout(2500);

// voice host
await nav("host");
let t = await main();
check(/connected|verbunden/i.test(t) && /2\.0\.0-alpha\.\d+/.test(t), "connected to the real proxy", t.match(/2\.0\.0-alpha\.\d+/)?.[0]);
check(/Friends over the internet|Freunde über das Internet/.test(t), "friends card on the voice host page");

// presets: Classic migrated, quality list
await nav("presets");
await page.waitForTimeout(1500);
t = await main();
check(/Classic/.test(t), "Classic preset present");
const classic = page.locator("article", { hasText: "Classic" });
await classic.getByRole("button", { name: /^(voices|stimmen)$/i }).click();
await page.waitForTimeout(800);
const dlg = page.locator("[role=dialog]");
const first = await dlg.locator("ul li").first().innerText();
check(/Voice of a movie-trailer narrator|Voice of a/.test(first), "Classic definitions come from the role names (migrated)", first.split("\n").slice(0, 3).join(" / "));
check((await dlg.locator("ul li").count()) === 30, "Classic has 30 roles");
await page.keyboard.press("Escape");
await page.waitForTimeout(300);
t = await main();
const odd = t.match(/(\d+) voices? may sound odd|(\d+) Stimmen? klingen? vielleicht seltsam/);
check(!!odd, "the quality notice shows odd voices", odd?.[0]);
if (odd) {
  await page.locator("main").getByRole("button", { name: /listen and decide|anhören und entscheiden/i }).click();
  await page.waitForTimeout(1200);
  const rv = page.locator("[role=dialog]");
  const rvt = await rv.innerText();
  check((await rv.locator("[data-review]").count()) >= 1 && /Hz|dB|%/.test(rvt), "review list with measured reasons", `${await rv.locator("[data-review]").count()} voices`);
  const unchecked = rvt.match(/(\d+) computed voices? (has|have) no quality report|(\d+) berechnete Stimmen? ha(t|ben) noch keinen/);
  console.log("      unchecked:", unchecked?.[0] ?? "none");
  await page.keyboard.press("Escape");
}

// jobs on the proxy
t = await main();
check(/Computing|Berechnung/.test(t), "job list shows the computing on the proxy", (t.match(/Everyday Neighbors[^\n]*|[A-Z][a-z]+ [A-Z][a-z]+ ?[^\n]*\d+ (of|von) \d+[^\n]*/) || [""])[0].slice(0, 80));

// live: one message read aloud
if (speak) {
  await nav("live");
  await page.locator("main textarea").fill("Kurzer Test der Sprachausgabe, alles in Ordnung.");
  const t0 = Date.now();
  await page.locator("main").getByRole("button", { name: /^(send|senden)$/i }).click();
  let ok = false;
  for (let i = 0; i < 60 && !ok; i++) {
    await page.waitForTimeout(500);
    const lt = await main();
    ok = /Kurzer Test der Sprachausgabe/.test(lt) && !/(queued|in der Warteschlange)[^\n]*Kurzer Test/.test(lt);
  }
  check(ok, "a test message is spoken through the proxy", `${Date.now() - t0} ms until it left the queue`);
}

check(errors.length === 0, "no console errors", errors.slice(0, 3).join(" / "));
await browser.close();
process.exit(failed ? 1 : 0);
