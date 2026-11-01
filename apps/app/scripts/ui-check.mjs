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
check(navItems.length === 7, "seven pages in the navigation", navItems.join(" | "));
const nav = (name) => page.locator("nav button").nth(["live", "profiles", "viewers", "presets", "voices", "host", "settings"].indexOf(name)).click();

for (let i = 0; i < navItems.length; i++) {
  await page.locator("nav button").nth(i).click();
  await page.waitForTimeout(250);
  const text = await page.locator("main").innerText();
  // vue-i18n shows the key itself when a message is missing.
  const raw = text.match(/\b(common|nav|host|voices|live|profiles|settings|tips|app|presets|jobs|compute|editor|lang|viewers|quality|emotions|fx|friends|packs|downloads)\.[a-zA-Z.]+\b/g);
  check(!raw, `page "${navItems[i].trim()}" renders without raw i18n keys`, raw?.slice(0, 3).join(", ") ?? `${text.length} chars`);
}

// Tooltips: hovering an info button shows the explanation (settings page).
await nav("settings");
await page.locator(".tip-btn").first().hover();
await page.waitForTimeout(500);
check(await page.locator("#ttv-tip").isVisible(), "tooltip appears on hover");
{
  const ts = page.locator("main [data-translation-service]");
  check(/MyMemory/.test(await ts.innerText()) && /DeepL/.test(await ts.innerText()), "the translation service can be set up in the settings");
  await ts.locator("input[type=password]").fill("demo-key:fx");
  await ts.getByRole("button", { name: /^(Save|Speichern)$/ }).click();
  await page.waitForTimeout(300);
  check(/(key stored|Schlüssel gespeichert)/.test(await ts.locator("input[type=password]").getAttribute("placeholder")) && (await ts.getByRole("button", { name: /^(Remove|Entfernen)$/ }).count()) === 1, "a DeepL key is stored and can be removed");
}
{
  const gm = page.locator("main [data-measured]");
  check(/2\.6 s \(×0\.51\)/.test(await gm.innerText()), "the measured generation speeds are shown", await gm.innerText());
  await gm.getByRole("button").click();
  await page.waitForTimeout(1600);
  check(/2\.4 s \(×0\.50\)/.test(await gm.innerText()), "generation can be measured again", await gm.innerText());
}
{
  const rc = page.locator("main [data-remote]");
  await rc.getByRole("switch").first().click();
  await page.waitForTimeout(300);
  check(/127.0.0.1:8130/.test(await rc.locator("[data-remote-state]").innerText()) && (await rc.locator("code").count()) === 7 && /token=demoToken123/.test(await rc.locator("code").first().innerText()), "remote control shows the URLs for Stream Deck", await rc.locator("code").first().innerText().catch(() => ""));
  await rc.getByRole("button", { name: /^(New token|Neuer Token)/ }).click();
  await page.waitForTimeout(300);
  check(!/demoToken123/.test(await rc.locator("code").first().innerText()), "a new token replaces the old one");
}
{
  const hk = page.locator("main [data-hotkeys]");
  check(/Ctrl \+ Alt \+ S/.test(await hk.locator("[data-hotkey=skip]").innerText()), "hotkeys are shown readably", await hk.locator("[data-hotkey=skip]").innerText());
  await hk.locator("[data-hotkey=pause]").click();
  check(/Press the keys|Tasten drücken/.test(await hk.locator("[data-hotkey=pause]").innerText()), "a hotkey field listens after a click");
  await page.keyboard.press("Control+Shift+KeyP");
  await page.waitForTimeout(300);
  check(/Ctrl \+ Shift \+ P/.test(await hk.locator("[data-hotkey=pause]").innerText()), "pressing a combination sets the hotkey", await hk.locator("[data-hotkey=pause]").innerText());
  await hk.locator("[data-hotkey=clear]").click();
  await page.keyboard.press("Control+Alt+KeyS");
  await page.waitForTimeout(300);
  check(/already used|bereits/.test(await page.locator("body").innerText()) && /(not set|nicht gesetzt)/.test(await hk.locator("[data-hotkey=clear]").innerText()), "a combination can't be used twice");
}

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
check(/0\.52 s (per s of speech|pro s Sprache)/.test(await page.locator("main [data-speed]").innerText()), "host page shows how fast VCP speaks", await page.locator("main [data-speed]").innerText());

// Live: current message, queue and history with reasons; test message appears.
await nav("live");
await page.waitForTimeout(300);
const live = await page.locator("main").innerText();
check(live.includes("Mikey_TV") && /ignored user/.test(live), "live page shows the current message and filtered ones with a reason");
check(/(raid|Raid)/.test(await page.locator("main [data-event]").first().innerText().catch(() => "")), "announcements are marked in the history");
check((await page.locator("main [data-replay]").count()) >= 1, "spoken messages can be played again", String(await page.locator("main [data-replay]").count()));
check(/(joy|Freude) \(normal\)/.test(await page.locator("main [data-emotion]").first().innerText()), "the current message shows its emotion", await page.locator("main [data-emotion]").first().innerText());
check(/Hallo zusammen, welches Spiel ist das\?/.test(await page.locator("main [data-translation]").first().innerText()), "translated messages show the translation", await page.locator("main [data-translation]").first().innerText());
await page.locator("main textarea").fill("Hallo aus dem Test");
await page.locator("main").getByRole("button", { name: /^(send|senden)$/i }).click();
await page.waitForTimeout(300);
check((await page.locator("main").innerText()).includes("Hallo aus dem Test"), "test message shows up in the history");

// Friends over the internet: switch on, a name, check, invite, remove.
await nav("host");
await page.waitForTimeout(500);
const fr = page.locator("section", { hasText: /Friends over the internet|Freunde über das Internet/ });
check(/Mia/.test(await fr.innerText()), "the host sees friends with access", (await fr.innerText()).match(/Mia[^\n]*/)?.[0]);
await fr.getByRole("switch").first().click();
await page.waitForTimeout(500);
check(/port 8111 open|Port 8111 offen/.test(await fr.innerText()), "internet access can be switched on");
await fr.getByPlaceholder(/my-stream|mein-stream/).fill("my-stream");
await fr.getByPlaceholder(/DuckDNS/).fill("1a2b3c4d-1a2b-1a2b-1a2b-1a2b3c4d5e6f");
await fr.getByRole("button", { name: /^(save|speichern)$/i }).click();
await page.waitForTimeout(500);
check(/my-stream\.duckdns\.org points at|my-stream\.duckdns\.org zeigt jetzt/.test(await page.locator("body").innerText()), "DuckDNS is set up from the app");
await fr.getByRole("button", { name: /check from the internet|aus dem internet prüfen/i }).click();
await page.waitForTimeout(900);
check(/Reachable at my-stream\.duckdns\.org:8111|Erreichbar unter my-stream\.duckdns\.org:8111/.test(await fr.innerText()), "the reachability check reports the address");
await fr.getByPlaceholder(/who is it for|für wen/i).fill("Alex");
await fr.getByRole("button", { name: /create code|code erstellen/i }).click();
await page.waitForTimeout(500);
check(/ttv1:/.test(await fr.innerText()) && /Code for Alex|Code für Alex/.test(await fr.innerText()), "an invite code is shown once with its expiry");
page.once("dialog", (d) => d.accept());
await fr.getByRole("button", { name: /remove access|zugang entfernen/i }).first().click();
await page.waitForTimeout(500);
check(!/Mia/.test(await fr.innerText()), "a friend's access can be removed");

// Profiles: editor shows the channel and saves changes.
await nav("profiles");
await page.waitForTimeout(300);
const prof = await page.locator("main").innerText();
check(prof.includes("zcarecrow"), "profile editor shows the channel");
check(/(Computed|Berechnet): de 41 · en 30/.test(prof), "profile shows how much of its preset is computed", prof.match(/(Computed|Berechnet):[^\n]*/)?.[0] ?? "");
{
  const tc = page.locator("main [data-translate]");
  const tct = await tc.innerText();
  check(/Translate for me|Für mich übersetzen/.test(tct) && /MyMemory/.test(tct) && /(original, then translation|Original, dann Übersetzung)/.test(tct), "profiles can translate chat for the streamer");
  await tc.locator("input").last().fill("hola a todos!");
  await tc.getByRole("button", { name: /^(Translate|Übersetzen)$/ }).click();
  await page.waitForTimeout(900);
  check(/Hallo zusammen! · 0\.5 s/.test(await tc.locator("[data-tried]").innerText().catch(() => "")), "a translation can be tried in the profile", await tc.locator("[data-tried]").innerText().catch(() => "none"));
}
{
  const ev = page.locator("main [data-events]");
  check(/Announcements|Ansagen/.test(await ev.innerText()) && /Gifted subs|Verschenkte Abos/.test(await ev.innerText()), "announcements can be set per profile");
  await ev.locator("summary").click();
  await ev.locator("[data-try-event=raid]").click();
  await page.waitForTimeout(300);
  check(/The example is in the queue|Das Beispiel ist in der Warteschlange/.test(await page.locator("body").innerText()), "an example announcement can be heard");
}
check(/System voice when the voice host is down|Systemstimme, wenn der Stimmen-Host ausfällt/.test(prof), "a system voice can stand in when the voice host is down");
check(/Emotions from chat|Gefühle aus dem Chat/.test(prof) && /At most|Höchstens/.test(prof) && /(strong|stark)/.test(prof), "emotions from chat can be switched and capped per profile");
check((await page.locator("main [role=switch]", { hasText: /^(Voice effects|Stimmeffekte)$/ }).count()) + (await page.locator("main").getByText(/^(Voice effects|Stimmeffekte)$/).count()) >= 1, "voice effects can be switched per profile");

// Viewers: list with voices, reassign by preset voice name.
await nav("viewers");
await page.waitForTimeout(500);
const vw = await page.locator("main").innerText();
check(/Mikey_TV/.test(vw) && /LenaPlays/.test(vw), "viewer list shows who chatted");
check((await page.locator("main table select").count()) >= 2, "voices can be reassigned by name (preset mode)", String(await page.locator("main table select").count()));
check((await page.locator("main table select").first().locator("option").count()) === 31, "reassign offers the profile's 30 preset voices", String(await page.locator("main table select").first().locator("option").count()));
const fxSel = page.locator("main [data-viewer-fx]");
check((await fxSel.count()) === 4 && (await fxSel.first().locator("option").count()) === 11, "every viewer can get a voice effect", `${await fxSel.count()} selects, ${await fxSel.first().locator("option").count()} options`);
check(/Radio/.test(await fxSel.nth(1).locator("option:checked").innerText()) && /(Robot|Roboter)/.test(await fxSel.nth(2).locator("option:checked").innerText()), "the viewer list shows set and preset effects", `${await fxSel.nth(1).locator("option:checked").innerText()} / ${await fxSel.nth(2).locator("option:checked").innerText()}`);
await fxSel.first().selectOption("whisper");
await page.waitForTimeout(300);
check(/Mikey_TV (now speaks with the effect|spricht jetzt mit dem Effekt) (Whisper|Flüstern)/.test(await page.locator("body").innerText()), "setting an effect confirms it");

await nav("voices");
await page.waitForTimeout(300);
check((await page.locator("main [data-fx] option").count()) === 10, "voices can be tried with an effect", String(await page.locator("main [data-fx] option").count()));

// Ready-made voices: download instead of computing.
await nav("presets");
await page.waitForTimeout(500);
const tavern = page.locator("article", { hasText: "Fantasy Tavern" });
await tavern.getByRole("button", { name: /^(get|holen)$/i }).click();
await page.waitForTimeout(500);
let dl = page.locator("[role=dialog]");
check(/Download 30 voices|30 Stimmen herunterladen/.test(await dl.innerText()), "ready-made voices are offered per preset", (await dl.innerText()).split("\n").slice(-3).join(" / "));
await dl.getByRole("button", { name: /^(german|englisch)$/i }).click();
await page.waitForTimeout(150);
check(/Download 60 voices|60 Stimmen herunterladen/.test(await dl.innerText()), "more languages, more voices");
await dl.getByRole("button", { name: /download 60 voices|60 stimmen herunterladen/i }).click();
await page.waitForTimeout(6500);
const tv = await tavern.innerText();
check(/de 30/.test(tv) && /en 30/.test(tv) && !(await tavern.getByRole("button", { name: /^(get|holen)$/i }).count()), "downloaded voices are imported and the offer disappears", tv.replace(/\n/g, " ").slice(0, 120));

// Voice packs: import one (preview first), share voices.
await nav("presets");
await page.waitForTimeout(300);
await page.locator("main").getByRole("button", { name: /^(import pack|paket importieren)$/i }).click();
await page.waitForTimeout(500);
let pk = page.locator("[role=dialog]");
const pkText = await pk.innerText();
check(/Saturday Cartoons/.test(pkText) && /60 voices|60 Stimmen/.test(pkText) && /you have it|hast du/.test(pkText), "the import dialog shows what a pack holds", pkText.split("\n").slice(0, 3).join(" / "));
await pk.getByRole("button", { name: /^(add 60 voices|60 stimmen hinzufügen)$/i }).click();
await page.waitForTimeout(5000);
check(/Saturday Cartoons/.test(await page.locator("article", { hasText: "Saturday Cartoons" }).innerText()) && /de 30/.test(await page.locator("article", { hasText: "Saturday Cartoons" }).innerText()), "imported voices count as computed", (await page.locator("article", { hasText: "Saturday Cartoons" }).innerText()).replace(/\n/g, " ").slice(0, 120));
await page.locator("article", { hasText: "Saturday Cartoons" }).getByRole("button", { name: /^(voices|stimmen)$/i }).click();
await page.waitForTimeout(500);
pk = page.locator("[role=dialog]");
await pk.getByRole("button", { name: /^(select|auswählen)$/i }).click();
await page.waitForTimeout(200);
await pk.locator("ul li input[type=checkbox]").nth(0).check();
await pk.locator("ul li input[type=checkbox]").nth(1).check();
await pk.getByRole("button", { name: /share 2 selected|2 ausgewählte teilen/i }).click();
await page.waitForTimeout(500);
const ex = page.locator("[role=dialog]").last();
check(/2 selected voices|2 ausgewählte Stimmen/.test(await ex.innerText()), "selected voices go into a pack of their own");
await ex.getByRole("button", { name: /save pack|paket speichern/i }).click();
await page.waitForTimeout(1200);
check(/Pack saved: 2 voices|Paket gespeichert: 2 Stimmen/.test(await page.locator("body").innerText()) && (await page.getByRole("button", { name: /show in folder|im ordner zeigen/i }).count()) === 1, "saving a pack says what's in it and offers the folder");
await page.keyboard.press("Escape");
await page.waitForTimeout(200);
await page.keyboard.press("Escape");
await page.waitForTimeout(200);

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
await page.locator("main").getByRole("button", { name: /^(check voices|stimmen prüfen)$/i }).click();
await page.waitForTimeout(1000);
check(/measured for loudness|Lautstärke gemessen/.test(await page.locator("body").innerText()), "checking the voices reports what it repaired");

// Quality check: odd voices are listed; listen, keep, improve, check the rest.
pre = await page.locator("main").innerText();
check(/4 voices may sound odd|4 Stimmen klingen vielleicht seltsam/.test(pre), "the presets page says how many voices may sound odd");
check(/4 to check|4 prüfen/.test(await page.locator("article", { hasText: "Cinematic" }).innerText()), "the preset card shows its voices to check");
await page.locator("main").getByRole("button", { name: /listen and decide|anhören und entscheiden/i }).click();
await page.waitForTimeout(500);
let rv = page.locator("[role=dialog]");
check((await rv.locator("[data-review]").count()) === 4, "the review list shows the odd voices", String(await rv.locator("[data-review]").count()));
const rvText = await rv.innerText();
check(/Pitch high for the role \(375 Hz\)|Hohe Tonlage für die Rolle \(375 Hz\)/.test(rvText) && /Sounds different in English|Klingt auf Englisch anders/.test(rvText), "issues are explained in words with the measured values");
check(/[0-9]+ computed voices have no quality report|[0-9]+ berechnete Stimmen haben noch keinen/.test(rvText), "voices without a report are counted");
await rv.locator("[data-review]").first().getByRole("button", { name: /original/i }).click();
await page.waitForTimeout(300);
check((await rv.locator("[data-review]").first().locator("[aria-label]").filter({ hasText: "" }).count()) > 0, "the original sample can be played");
await page.waitForTimeout(1200);
await rv.locator("[data-review]").nth(3).getByRole("button", { name: /^(keep|behalten)$/i }).click();
await page.waitForTimeout(600);
check((await rv.locator("[data-review]").count()) === 3, "a kept voice leaves the list", String(await rv.locator("[data-review]").count()));
await rv.getByRole("button", { name: /^(improve all|alle verbessern)$/i }).click();
await page.waitForTimeout(400);
check(/Improving|Verbessere/.test(await page.locator("body").innerText()), "improving starts a job");
await page.waitForTimeout(2500);
check((await rv.locator("[data-review]").count()) === 0 && /No voice sounds odd|Keine Stimme klingt seltsam/.test(await rv.innerText()), "improved voices leave the list", String(await rv.locator("[data-review]").count()));
await rv.getByRole("button", { name: /^(check them|jetzt prüfen)$/i }).click();
await page.waitForTimeout(9000);
check(!/no quality report yet|noch keinen Qualitätsbericht/.test(await rv.innerText()), "checking gives every voice a report");
await rv.getByRole("button", { name: /^(close|schließen)$/i }).last().click();
await page.waitForTimeout(300);
pre = await page.locator("main").innerText();
check(/Quality check|Qualitätsprüfung/.test(pre) && /improved|verbessert/.test(pre), "the job list shows the quality check and the improvement");

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
await dlg.locator("ul li").first().getByRole("button", { name: /^(listen|anhören)$/i }).click();
await page.waitForTimeout(300);
check((await dlg.locator("ul li").first().locator(".spin").count()) === 1, "listening to a computed voice shows progress");
await page.waitForTimeout(1500);
check((await dlg.locator("ul li").first().getByRole("button", { name: /^(design again|neu entwerfen)$/i }).count()) === 1, "a computed voice can be designed again");
await dlg.locator("ul li").first().getByRole("button", { name: /^(design again|neu entwerfen)$/i }).click();
await page.waitForTimeout(200);
check((await dlg.locator("ul li").first().locator("[data-redo-menu] button").count()) === 3, "designing again asks: automatically or let me pick");
await dlg.locator("ul li").first().getByRole("button", { name: /^(automatically|automatisch)/i }).click();
await page.waitForTimeout(400);
check(/being designed|wird entworfen/.test(await dlg.locator("ul li").first().innerText()), "a voice being designed again says so");
await page.waitForTimeout(2500);
const back = dlg.locator("ul li").first().getByRole("button", { name: /^(listen|anhören)$/i });
check(!(await back.isDisabled()) && !/being designed|wird entworfen/.test(await dlg.locator("ul li").first().innerText()), "the redone voice can be played without reopening the dialog");
// let me pick: three takes, one recommended, pick one
{
  const row = dlg.locator("ul li").nth(1);
  await row.getByRole("button", { name: /^(design again|neu entwerfen)$/i }).click();
  await page.waitForTimeout(200);
  await row.getByRole("button", { name: /^(let me pick|ich wähle)/i }).click();
  await page.waitForTimeout(1600);
  check(/3 takes to pick|3 Versuche zur Auswahl/.test(await row.innerText()), "takes to pick from are announced on the voice", (await row.innerText()).split("\n")[0]);
  await row.locator("[data-takes]").click();
  await page.waitForTimeout(200);
  const list = row.locator("[data-take-list]");
  const lt = await list.innerText();
  check((await list.getByRole("button", { name: /^(use this take|diesen versuch nehmen)$/i }).count()) === 3 && /(recommended|empfohlen)/.test(lt), "the takes can be heard and one is recommended", lt.replace(/\n/g, " ").slice(0, 120));
  await list.getByRole("button", { name: /^(use this take|diesen versuch nehmen)$/i }).nth(1).click();
  await page.waitForTimeout(500);
  check(/speaks with take 2 now|spricht jetzt mit Versuch 2/.test(await page.locator("body").innerText()) && !(await row.locator("[data-takes]").count()), "picking a take makes it the voice");
}
await dlg.getByRole("button", { name: /^(compute|berechnen)$/i }).click();
await page.waitForTimeout(500);
const cd = page.locator("[role=dialog]");
const cdText = await cd.innerText();
check(/Quality check|Qualitätsprüfung/.test(cdText) && /Improve odd voices automatically|Seltsame Stimmen automatisch verbessern/.test(cdText), "the compute dialog offers the quality check");
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

// A friend: joining with an invite code, then guest mode.
const lang = (query.match(/lang=(\w+)/) || [])[1];
const q2 = (extra) => `?theme=dark${lang ? `&lang=${lang}` : ""}&${extra}`;
await page.goto(base + q2("nohost"));
await page.waitForSelector("nav button");
await nav("host");
await page.waitForTimeout(400);
await page.locator("main").getByRole("button", { name: /a friend.s voice host|stimmen-host eines freundes/i }).click();
await page.locator("main textarea").fill("ttv1:eyJhZGRyZXNzIjoibXktc3RyZWFtLmR1Y2tkbnMub3JnOjgxMTEifQ");
await page.waitForTimeout(400);
check(/ALEX-PC/.test(await page.locator("main").innerText()) && !(await page.locator("main").getByRole("button", { name: /^(join|beitreten)$/i }).isDisabled()), "an invite code shows whose voice host it is before joining");
await page.goto(base + q2("guest"));
await page.waitForSelector("nav button");
await nav("presets");
await page.waitForTimeout(500);
const gp = await page.locator("main").innerText();
check(/You use .*voices|Du nutzt die Stimmen/.test(gp) && !(await page.locator("main").getByRole("button", { name: /^(compute|berechnen)$/i }).count()), "guests can't compute on a friend's voice host");
await nav("host");
await page.waitForTimeout(400);
const gh = await page.locator("main").innerText();
check(/Used today|Heute genutzt/.test(gh) && /4,200|4\.200/.test(gh) && !/Friends over the internet|Freunde über das Internet/.test(gh), "guests see their quota, not the friends settings");

// approval mode: a message waits until it is approved
await page.goto(base + query + "&approval");
await page.waitForSelector("nav button");
await nav("live");
await page.waitForTimeout(400);
{
  const pend = page.locator("main [data-pending]");
  check(/Grumpy_Gus/.test(await pend.innerText().catch(() => "")), "approval mode lists waiting messages");
  await pend.getByRole("button", { name: /^(Read it|Vorlesen)$/ }).click();
  await page.waitForTimeout(300);
  check(!(await page.locator("main [data-pending]").count()) && /Grumpy_Gus/.test(await page.locator("main").innerText()), "an approved message moves to the queue");
}
await nav("profiles");
await page.waitForTimeout(300);
check(/Approve messages before they are read|Nachrichten vor dem Vorlesen freigeben/.test(await page.locator("main [data-approval]").innerText()), "profiles can require approval");

check(errors.length === 0, "no console errors", errors.slice(0, 3).join(" / "));
await browser.close();
process.exit(failed ? 1 : 0);
