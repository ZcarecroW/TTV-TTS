// Uploads a preset's voice packs as soon as that preset is 100 % finished on
// the voice host PC: all its voices in all languages of the plan, checked, and
// nothing left to improve. Presets that are only partly finished stay back.
//
// The plan runs on the VCP PC, the GitHub login is here: a scheduled task runs
// this every hour. It looks over SSH (host "vcp-pc"), copies the voices of
// finished presets over, builds and uploads their packs
// (scripts/build-voice-packs.mjs, which picks up where it stopped), and
// removes the task when every preset is online.
//   node scripts/publish-when-ready.mjs                what the task runs
//   node scripts/publish-when-ready.mjs --status       only says what's finished
//   node scripts/publish-when-ready.mjs --dry --preset <id>
//                                                      builds one preset's packs,
//                                                      however far it is, no upload
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { appendFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const dry = process.argv.includes("--dry");
const statusOnly = process.argv.includes("--status");
const forced = process.argv.includes("--preset") ? process.argv[process.argv.indexOf("--preset") + 1] : null;
const work = join(root, ".work", "publish");
const src = join(work, "src");
const marker = join(work, "published.json");
mkdirSync(work, { recursive: true });
// Git's OpenSSH: that's the one the firewall of this PC lets out
const SSH = "C:/Program Files/Git/usr/bin/ssh.exe";
const SCP = "C:/Program Files/Git/usr/bin/scp.exe";
const TASK = "TTV-TTS\\Publish voice packs";
const ENGINE_REMOTE = "C:/Users/zcrow/AppData/Roaming/io.github.zcarecrow.ttvtts.proxy/engine";
const REMOTE_TAR = "C:/TTV-dev/packs-src.tar";
const REMOTE_LIST = "C:/TTV-dev/packs-list.txt";
/** What crates/engine/src/quality.rs and plan.rs use. */
const FLAG = 0.75;
const MAX_IMPROVES = 2;

const log = (m) => {
  const line = `${new Date().toISOString().slice(0, 19)} ${m}`;
  console.log(line);
  appendFileSync(join(work, "publish.log"), `${line}\n`);
};
const ssh = (cmd, timeout = 120_000) => execFileSync(SSH, ["vcp-pc", cmd], { encoding: "utf8", timeout, stdio: ["ignore", "pipe", "pipe"] }).trim();
const unixPath = (p) => p.replaceAll("\\", "/").replace(/^([A-Za-z]):/, "/$1");
const scp = (from, to, timeout = 7_200_000) => execFileSync(SCP, ["-q", from, to], { stdio: "inherit", timeout });

/** VoiceDef::hash: sha256(gender, 0, age, description) → "v" + 12 hex. */
const defHash = (d) => "v" + createHash("sha256").update(Buffer.concat([Buffer.from(d.gender), Buffer.from([0, d.age]), Buffer.from(d.description.trim())])).digest("hex").slice(0, 12);

const entries = (file) => {
  const m = new Map();
  for (const l of readFileSync(file, "utf8").split("\n")) {
    if (!l.trim()) continue;
    const o = JSON.parse(l);
    if (o.op === "add") m.set(o.entry.key, o.entry);
    else m.delete(o.key);
  }
  return m;
};

/** What's still missing for a preset to be 100 % finished. */
function missing(preset, plan, voices) {
  const left = { voices: 0, unchecked: 0, odd: 0 };
  for (const lang of plan.request.languages) {
    for (const d of preset.defs) {
      const key = `${defHash(d.def)}:${lang}`;
      const e = voices.get(key);
      if (!e) left.voices++;
      else if (!e.quality) left.unchecked++;
      else if (e.quality.score < FLAG && !e.quality.kept && (plan.improves[key] ?? 0) < MAX_IMPROVES) left.odd++;
    }
  }
  return left;
}

// a small look first: what's finished?
const meta = join(work, "meta");
mkdirSync(meta, { recursive: true });
try {
  for (const f of ["plan.json", "library.jsonl", "archive.jsonl"]) scp(`vcp-pc:${ENGINE_REMOTE}/${f}`, unixPath(join(meta, f)), 300_000);
} catch (e) {
  // the VCP PC is off or asleep, or there's no plan: next hour
  log(`nothing to read from the VCP PC: ${String(e.message).split("\n")[0]}`);
  process.exit(0);
}
const plan = JSON.parse(readFileSync(join(meta, "plan.json"), "utf8"));
const voices = new Map([...entries(join(meta, "archive.jsonl")), ...entries(join(meta, "library.jsonl"))]);
const published = new Set(existsSync(marker) ? JSON.parse(readFileSync(marker, "utf8")) : []);
const report = plan.request.presets.map((p) => ({ id: p.id, left: missing(p, plan, voices) }));
const finished = report.filter((r) => !r.left.voices && !r.left.unchecked && !r.left.odd).map((r) => r.id);
if (statusOnly) {
  for (const r of report) console.log(`${r.id.padEnd(22)} ${published.has(r.id) ? "online" : finished.includes(r.id) ? "finished" : `${r.left.voices} voices missing, ${r.left.unchecked} unchecked, ${r.left.odd} to improve`}`);
  process.exit(0);
}
const ready = forced ? [forced] : finished.filter((id) => !published.has(id));
if (!ready.length) {
  log(`${finished.length} of ${report.length} presets finished, ${published.size} online, nothing new`);
  process.exit(0);
}

log(`finished: ${ready.join(", ")}`);
// only these presets' voices: the whole archive grows to many gigabytes
rmSync(src, { recursive: true, force: true });
mkdirSync(src, { recursive: true });
const list = [];
for (const p of plan.request.presets.filter((x) => ready.includes(x.id))) {
  for (const lang of plan.request.languages) {
    for (const d of p.defs) {
      const key = `${defHash(d.def)}:${lang}`;
      if (voices.has(key)) list.push(`samples/${key.replace(":", "_")}.flac`);
    }
  }
}
const local = join(work, "list.txt");
writeFileSync(local, `${list.join("\n")}\n`);
log(`copying ${list.length} voices`);
scp(unixPath(local), `vcp-pc:${REMOTE_LIST}`, 120_000);
ssh(`icacls ${REMOTE_LIST} /reset /Q | Out-Null; tar.exe -cf ${REMOTE_TAR} -C $(Join-Path $env:APPDATA "io.github.zcarecrow.ttvtts.proxy\\engine") -T ${REMOTE_LIST}; icacls ${REMOTE_TAR} /reset /Q | Out-Null`, 3_600_000);
scp(`vcp-pc:${REMOTE_TAR}`, unixPath(join(work, "src.tar")));
ssh(`Remove-Item ${REMOTE_TAR}, ${REMOTE_LIST} -Force`);
execFileSync("tar", ["-xf", "src.tar", "-C", "src"], { cwd: work, stdio: "inherit", timeout: 3_600_000 });
rmSync(join(work, "src.tar"), { force: true });
for (const f of ["library.jsonl", "archive.jsonl"]) writeFileSync(join(src, f), readFileSync(join(meta, f)));

log("building the packs");
const build = [join(root, "scripts", "build-voice-packs.mjs"), ...(dry ? ["--dry"] : [])];
execFileSync(process.execPath, build, { cwd: root, stdio: "inherit", env: { ...process.env, TTV_FROM: src, TTV_ONLY: ready.join(",") } });

rmSync(src, { recursive: true, force: true });
if (dry) {
  log("dry run finished: nothing uploaded");
  process.exit(0);
}
rmSync(join(root, ".work", "voice-packs"), { recursive: true, force: true });
for (const id of ready) published.add(id);
writeFileSync(marker, JSON.stringify([...published]));
log(`online: ${ready.join(", ")} (${published.size} of ${report.length} presets)`);
if (report.every((r) => published.has(r.id))) {
  try {
    execFileSync("schtasks", ["/Delete", "/TN", TASK, "/F"], { stdio: "ignore" });
    log("every preset is online: the scheduled task removed itself");
  } catch {
    log("every preset is online; couldn't remove the scheduled task (harmless)");
  }
}
