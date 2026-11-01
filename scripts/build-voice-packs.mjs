// Builds the ready-made voice packs from a proxy's library and publishes
// them with their catalog on the GitHub release "voice-packs".
//   TTV_PROXY=192.168.178.25:8110 TTV_TOKEN=<key or device token> node scripts/build-voice-packs.mjs [--dry]
// One pack per built-in preset, language and complete slice of 30 voices
// (1–30, 31–60, …). Needs TTV-TTS Proxy 2.0.0-alpha.10+ (pack export) and gh.
// The proxy's certificate is self-signed: run with NODE_TLS_REJECT_UNAUTHORIZED=0
// on the LAN, or pin it.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const proxy = process.env.TTV_PROXY;
const token = process.env.TTV_TOKEN;
if (!proxy || !token) throw new Error("set TTV_PROXY and TTV_TOKEN");
const dry = process.argv.includes("--dry");
const repo = "ZcarecroW/TTV-TTS";
const TAG = "voice-packs";
const SLICE = 30;
const out = join(root, ".work", "voice-packs");
mkdirSync(out, { recursive: true });

const ORDER = ["cinematic", "everyday-neighbors", "fantasy-tavern", "starship-crew", "noir-city", "saturday-cartoons", "high-seas", "royal-court", "midnight-horror", "arena-casters", "on-air", "wild-planet", "storytime", "frontier-west", "neon-streets", "olympus", "capes-and-masks", "grand-stage", "quest-givers", "calm-and-cozy"];
const H = { Authorization: `Bearer ${token}`, "Content-Type": "application/json" };
const api = (path, init) => fetch(`https://${proxy}/ttv/v1${path}`, { headers: H, ...init });

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

const lib = await (await api("/library")).json();
const have = new Set(lib.entries.map((e) => e.key));
const app = (await (await api("/info")).json()).version;
const packs = [];
for (const id of ORDER) {
  const p = preset(id);
  const langs = [...new Set(lib.entries.map((e) => e.lang))].sort();
  for (const lang of langs) {
    for (let from = 0; from + SLICE <= p.defs.length; from += SLICE) {
      const slice = p.defs.slice(from, from + SLICE);
      if (!slice.every((d) => have.has(`${defHash(d)}:${lang}`))) break;
      const file = `${id}-${lang}-${from + 1}-${from + SLICE}.ttvpack`;
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
      packs.push({ file, preset: id, lang, from: from + 1, to: from + SLICE, voices: SLICE, bytes: bytes.length, sha256 });
      console.log(`${file}: ${(bytes.length / 1048576).toFixed(1)} MB in ${((Date.now() - t) / 1000).toFixed(1)} s`);
    }
  }
}
const catalog = { version: 1, updated: new Date().toISOString().slice(0, 10), app, packs };
writeFileSync(join(out, "catalog.json"), JSON.stringify(catalog, null, 1));
const total = packs.reduce((n, p) => n + p.bytes, 0);
console.log(`${packs.length} packs, ${packs.reduce((n, p) => n + p.voices, 0)} voices, ${(total / 1048576).toFixed(0)} MB`);
if (dry) process.exit(0);

const gh = (...args) => execFileSync("gh", args, { stdio: ["ignore", "pipe", "inherit"], encoding: "utf8" });
let exists = true;
try {
  gh("release", "view", TAG, "-R", repo, "--json", "tagName");
} catch {
  exists = false;
}
if (!exists) {
  gh("release", "create", TAG, "-R", repo, "--prerelease", "--title", "Ready-made voices", "--notes", "Voice packs for the built-in presets, made with Voice Creator Pro and checked by TTV-TTS's quality check. TTV-TTS downloads them from here (Presets → Get); you can also import a .ttvpack by hand.");
}
// packs first, catalog last: the app never sees a catalog entry without its file
for (const p of packs) gh("release", "upload", TAG, "-R", repo, "--clobber", join(out, p.file));
gh("release", "upload", TAG, "-R", repo, "--clobber", join(out, "catalog.json"));
console.log("published", statSync(join(out, "catalog.json")).size, "byte catalog");
