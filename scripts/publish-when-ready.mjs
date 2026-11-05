// Uploads the ready-made voice packs as soon as the automatic plan on the
// voice host PC is finished. The plan runs on the VCP PC, the GitHub login is
// here: a scheduled task runs this every hour, it looks over SSH (host "vcp-pc")
// and does nothing until the plan is done. Then it copies the voices over,
// builds and uploads the packs (scripts/build-voice-packs.mjs), which can pick
// up where it stopped, and removes the task.
//   node scripts/publish-when-ready.mjs            what the task runs
//   node scripts/publish-when-ready.mjs --dry      everything but the upload,
//                                                  whether the plan is done or not
import { execFileSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const dry = process.argv.includes("--dry");
const work = join(root, ".work", "publish");
const src = join(work, "src");
mkdirSync(work, { recursive: true });
// Git's OpenSSH: that's the one the firewall of this PC lets out
const SSH = "C:/Program Files/Git/usr/bin/ssh.exe";
const SCP = "C:/Program Files/Git/usr/bin/scp.exe";
const TASK = "TTV-TTS\\Publish voice packs";
const ENGINE = '(Join-Path $env:APPDATA "io.github.zcarecrow.ttvtts.proxy\\engine")';
const REMOTE_TAR = "C:\\TTV-dev\\packs-src.tar";

const log = (m) => {
  const line = `${new Date().toISOString().slice(0, 19)} ${m}`;
  console.log(line);
  appendFileSync(join(work, "publish.log"), `${line}\n`);
};
const ssh = (cmd, timeout = 120_000) => execFileSync(SSH, ["vcp-pc", cmd], { encoding: "utf8", timeout, stdio: ["ignore", "pipe", "pipe"] }).trim();

let state;
try {
  state = ssh(`$p = Join-Path ${ENGINE} "plan.json"; if (-not (Test-Path $p)) { "none" } elseif (Select-String -Quiet -SimpleMatch '"state":"done"' -Path $p) { "done" } else { "running" }`);
} catch (e) {
  // the VCP PC is off, or asleep: next hour
  log(`VCP PC not reachable: ${String(e.message).split("\n")[0]}`);
  process.exit(0);
}
if (!dry && state !== "done") {
  log(`plan: ${state}`);
  process.exit(0);
}
const marker = join(work, "published.json");
if (!dry && existsSync(marker)) {
  log("published already");
  process.exit(0);
}

log("copying the voices from the VCP PC");
rmSync(src, { recursive: true, force: true });
mkdirSync(src, { recursive: true });
ssh(`tar.exe -cf ${REMOTE_TAR} -C ${"$($e = " + ENGINE + "; $e)"} samples library.jsonl archive.jsonl; icacls ${REMOTE_TAR} /reset /Q | Out-Null`, 3_600_000);
execFileSync(SCP, ["-q", `vcp-pc:${REMOTE_TAR.replaceAll("\\", "/")}`, join(work, "src.tar").replaceAll("\\", "/").replace(/^([A-Za-z]):/, "/$1")], { stdio: "inherit", timeout: 7_200_000 });
ssh(`Remove-Item ${REMOTE_TAR} -Force`);
execFileSync("tar", ["-xf", "src.tar", "-C", "src"], { cwd: work, stdio: "inherit", timeout: 3_600_000 });
rmSync(join(work, "src.tar"), { force: true });

log("building the packs");
const build = [join(root, "scripts", "build-voice-packs.mjs"), ...(dry ? ["--dry"] : [])];
execFileSync(process.execPath, build, { cwd: root, stdio: "inherit", env: { ...process.env, TTV_FROM: src } });

if (dry) {
  log("dry run finished: nothing uploaded");
  process.exit(0);
}
writeFileSync(marker, JSON.stringify({ at: new Date().toISOString() }));
log("all packs are online");
rmSync(src, { recursive: true, force: true });
rmSync(join(root, ".work", "voice-packs"), { recursive: true, force: true });
try {
  execFileSync("schtasks", ["/Delete", "/TN", TASK, "/F"], { stdio: "ignore" });
  log("the scheduled task removed itself");
} catch {
  log("couldn't remove the scheduled task (harmless: it ends at once from now on)");
}
