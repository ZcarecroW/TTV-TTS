// Builds the ready-made voice packs from a proxy's library (and its archive)
// and publishes them: the first ones on the GitHub release "voice-packs",
// newer ones on one release per language ("voice-packs-de", … – a release
// holds at most 1000 files). "voice-packs" carries catalog-v2.json (all packs
// and their releases, TTV-TTS 2.1+) and catalog.json (only its own, 2.0).
// Packs whose voices didn't change since they were published (signature)
// aren't exported or uploaded again.
//   TTV_PROXY=192.168.178.25:8110 TTV_TOKEN=<key or device token> node scripts/build-voice-packs.mjs [--dry]
//   TTV_VIA_APP=1 node scripts/build-voice-packs.mjs [--dry]
// One pack per built-in preset, language and complete slice of 30 voices
// (1–30, 31–60, …). Needs TTV-TTS Proxy 2.0.0-alpha.10+ (pack export) and gh.
// The proxy's certificate is self-signed: run with NODE_TLS_REJECT_UNAUTHORIZED=0
// on the LAN, or pin it. TTV_VIA_APP=1 goes through a running development
// build of the app instead (remote debugging on :9333, `debug_proxy_fetch`),
// with its pairing – no key or token needed here.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const viaApp = process.env.TTV_VIA_APP === "1";
const proxy = process.env.TTV_PROXY;
const token = process.env.TTV_TOKEN;
if (!viaApp && (!proxy || !token)) throw new Error("set TTV_PROXY and TTV_TOKEN, or TTV_VIA_APP=1");
const dry = process.argv.includes("--dry");
const repo = "ZcarecroW/TTV-TTS";
const TAG = "voice-packs";
/** Packs go to one release per language (GitHub allows 1000 files per
 * release); the first ones stay on "voice-packs", where TTV-TTS 2.0 finds them. */
const tagFor = (lang) => `voice-packs-${lang}`;
const SLICE = 30;
const out = join(root, ".work", "voice-packs");
mkdirSync(out, { recursive: true });

const ORDER = ["cinematic", "everyday-neighbors", "fantasy-tavern", "starship-crew", "noir-city", "saturday-cartoons", "high-seas", "royal-court", "midnight-horror", "arena-casters", "on-air", "wild-planet", "storytime", "frontier-west", "neon-streets", "olympus", "capes-and-masks", "grand-stage", "quest-givers", "calm-and-cozy"];
const H = { Authorization: `Bearer ${token}`, "Content-Type": "application/json" };
let api = (path, init) => fetch(`https://${proxy}/ttv/v1${path}`, { headers: H, ...init });
if (viaApp) {
  const { chromium } = await import("playwright-core");
  const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
  const page = browser.contexts().flatMap((c) => c.pages()).find((p) => /localhost:1440|tauri\.localhost/.test(p.url()));
  process.on("exit", () => browser.close().catch(() => {}));
  // a Response-like answer from the app's passthrough
  api = async (path, init = {}) => {
    const a = await page.evaluate(([m, p, b]) => window.__TAURI_INTERNALS__.invoke("debug_proxy_fetch", { method: m, path: p, body: b }), [init.method ?? "GET", `/ttv/v1${path}`, init.body ?? null]);
    const bytes = Buffer.from(a.body, "base64");
    return { ok: a.status >= 200 && a.status < 300, status: a.status, json: async () => JSON.parse(bytes.toString("utf8")), text: async () => bytes.toString("utf8"), arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.length) };
  };
}

/** VoiceDef::hash: sha256(gender, 0, age, description) → "v" + 12 hex. */
const defHash = (d) => "v" + createHash("sha256").update(Buffer.concat([Buffer.from(d.gender), Buffer.from([0, d.age]), Buffer.from(d.description.trim())])).digest("hex").slice(0, 12);

function preset(id) {
  const text = readFileSync(join(root, "apps/app/src-tauri/presets", `${id}.txt`), "utf8");
  const lines = text.split(/\r?\n/);
  const name = (lines.find((l) => l.startsWith("name:")) || "").slice(5).trim();
  const defs = lines.slice(lines.indexOf("---") + 1).filter((l) => l.includes("|")).map((l) => {
    const [n, g, age, description] = l.split("|").map((s) => s.trim());
    return { name: n, gender: g, age: Number(age), description };
  });
  return { id, name, text, defs };
}

// computed voices: in VCP (library) and the ones that only live in the
// engine's archive (ready-made packs, proxy 2.1.0+)
const lib = await (await api("/library")).json();
const archived = await api("/library/archived").then((r) => (r.ok ? r.json() : { entries: [] }), () => ({ entries: [] }));
lib.entries.push(...archived.entries);
const byKey = new Map(lib.entries.map((e) => [e.key, e]));
console.log(`${lib.entries.length} computed voices (${archived.entries.length} archived)`);
const app = (await (await api("/info")).json()).version;
const gh = (...args) => execFileSync("gh", args, { stdio: ["ignore", "pipe", "inherit"], encoding: "utf8", maxBuffer: 64 << 20 });
const assets = (tag) => {
  try {
    return new Map(JSON.parse(gh("api", `repos/${repo}/releases/tags/${tag}`, "--jq", "[.assets[] | [.name, .digest]]")).map(([n, d]) => [n, d]));
  } catch {
    return null;
  }
};
const legacy = assets(TAG) ?? new Map();
/** What the published catalogs say about each pack (to skip unchanged ones). */
const previous = new Map();
for (const name of ["catalog.json", "catalog-v2.json"]) {
  try {
    for (const e of JSON.parse(gh("release", "download", TAG, "-R", repo, "-p", name, "-O", "-")).packs) previous.set(e.file, e);
  } catch {}
}
/** What a slice's voices are: changes when one is designed again or improved. */
const signature = (keys) =>
  createHash("sha256")
    .update(JSON.stringify(keys.map((k) => { const e = byKey.get(k); return [k, e.voice_id, e.gain, e.level, e.duration, e.quality?.score ?? null]; })))
    .digest("hex")
    .slice(0, 16);
const packs = [];
let reused = 0;
for (const id of ORDER) {
  const p = preset(id);
  const langs = [...new Set(lib.entries.map((e) => e.lang))].sort();
  for (const lang of langs) {
    for (let from = 0; from + SLICE <= p.defs.length; from += SLICE) {
      const slice = p.defs.slice(from, from + SLICE);
      const keys = slice.map((d) => `${defHash(d)}:${lang}`);
      if (!keys.every((k) => byKey.has(k))) break;
      const file = `${id}-${lang}-${from + 1}-${from + SLICE}.ttvpack`;
      const sig = signature(keys);
      const old = previous.get(file);
      // published already and unchanged (packs from before signatures: as published)
      if (old && (old.sig === sig || (!old.sig && legacy.get(file) === `sha256:${old.sha256}`))) {
        packs.push({ ...old, sig, tag: old.tag ?? TAG });
        reused++;
        continue;
      }
      const body = {
        title: `${p.name} – voices ${from + 1}–${from + SLICE} (${lang})`,
        presets: [{ id, name: p.name, builtin: true, text: p.text }],
        voices: slice.map((d, i) => ({ key: `${defHash(d)}:${lang}`, preset_id: id, index: from + i, name: d.name, description: d.description })),
      };
      const t = Date.now();
      const r = await api("/packs/export", { method: "POST", body: JSON.stringify(body) });
      if (!r.ok) throw new Error(`${file}: ${r.status} ${await r.text()}`);
      const bytes = Buffer.from(await r.arrayBuffer());
      writeFileSync(join(out, file), bytes);
      const sha256 = createHash("sha256").update(bytes).digest("hex");
      packs.push({ file, preset: id, lang, from: from + 1, to: from + SLICE, voices: SLICE, bytes: bytes.length, sha256, sig });
      console.log(`${file}: ${(bytes.length / 1048576).toFixed(1)} MB in ${((Date.now() - t) / 1000).toFixed(1)} s`);
    }
  }
}
console.log(`${packs.length - reused} packs exported, ${reused} unchanged`);
for (const p of packs) p.tag ??= legacy.has(p.file) ? TAG : tagFor(p.lang);
const updated = new Date().toISOString().slice(0, 10);
writeFileSync(join(out, "catalog-v2.json"), JSON.stringify({ version: 2, updated, app, packs }, null, 1));
// TTV-TTS 2.0 reads catalog.json and downloads from "voice-packs" only
writeFileSync(join(out, "catalog.json"), JSON.stringify({ version: 1, updated, app, packs: packs.filter((p) => p.tag === TAG).map(({ tag, ...p }) => p) }, null, 1));
const total = packs.reduce((n, p) => n + p.bytes, 0);
console.log(`${packs.length} packs, ${packs.reduce((n, p) => n + p.voices, 0)} voices, ${(total / 1048576).toFixed(0)} MB`);
if (dry) process.exit(0);

const online = new Map([[TAG, legacy]]);
const release = (tag) => {
  if (!online.has(tag)) {
    let a = assets(tag);
    if (!a) {
      const lang = tag.slice(TAG.length + 1);
      gh("release", "create", tag, "-R", repo, "--prerelease", "--title", `Ready-made voices (${lang})`, "--notes", `Voice packs of the built-in presets in the language "${lang}", made with Voice Creator Pro and checked by TTV-TTS's quality check. TTV-TTS downloads them (Presets → Ready-made voices); you can also import a .ttvpack by hand. The list of all packs is on the release "voice-packs".`);
      a = new Map();
    }
    online.set(tag, a);
  }
  return online.get(tag);
};
if (!legacy.size && !assets(TAG)) {
  gh("release", "create", TAG, "-R", repo, "--prerelease", "--title", "Ready-made voices", "--notes", "Voice packs for the built-in presets, made with Voice Creator Pro and checked by TTV-TTS's quality check. TTV-TTS downloads them from here and from the releases voice-packs-<language>; you can also import a .ttvpack by hand.");
}
// packs first, catalogs last: the app never sees a catalog entry without its file.
// A pack GitHub already has with the same sha256 isn't uploaded again.
let skipped = 0;
for (const p of packs) {
  if (release(p.tag).get(p.file) === `sha256:${p.sha256}`) {
    // unchanged (not exported again) or the same bytes
    skipped++;
    continue;
  }
  gh("release", "upload", p.tag, "-R", repo, "--clobber", join(out, p.file));
}
console.log(`${packs.length - skipped} packs uploaded, ${skipped} unchanged`);
gh("release", "upload", TAG, "-R", repo, "--clobber", join(out, "catalog.json"), join(out, "catalog-v2.json"));
console.log("published", statSync(join(out, "catalog-v2.json")).size, "byte catalog");
// the debugging connection to the app (TTV_VIA_APP) would keep Node running
process.exit(0);
