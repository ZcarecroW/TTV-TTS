// Builds the next preview of TTV-TTS and/or TTV-TTS Proxy and publishes it
// to the rolling GitHub pre-release "preview", where installed copies on the
// preview channel pick it up as an update.
//   node scripts/publish-preview.mjs              (both apps, bumps 2.0.0-alpha.N -> N+1)
//   node scripts/publish-preview.mjs --only proxy (or --only app)
//   node scripts/publish-preview.mjs --no-bump    (republish the current version)
// Needs: gh (logged in), the updater key (TTV_UPDATER_KEY_DIR, default
// ~/Downloads/TTV-TTS-updater-key), Rust and the npm workspace.
import { execFileSync } from "node:child_process";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = "ZcarecroW/TTV-TTS";
const keyDir = process.env.TTV_UPDATER_KEY_DIR ?? join(homedir(), "Downloads", "TTV-TTS-updater-key");
// Only npx needs a shell on Windows (it is a .cmd file); gh and git run directly.
const sh = (cmd, args, opts = {}) => execFileSync(cmd, args, { cwd: root, stdio: "inherit", shell: cmd === "npx" && process.platform === "win32", ...opts });
const out = (cmd, args) => execFileSync(cmd, args, { cwd: root, encoding: "utf8" }).trim();

const APPS = {
  proxy: { dir: "apps/proxy", product: "TTV-TTS Proxy", asset: "TTV-TTS-Proxy", manifest: "latest-proxy.json" },
  app: { dir: "apps/app", product: "TTV-TTS", asset: "TTV-TTS", manifest: "latest.json" },
};
const onlyIdx = process.argv.indexOf("--only");
const which = onlyIdx > 0 ? [process.argv[onlyIdx + 1]] : Object.keys(APPS);
for (const w of which) if (!APPS[w]) throw new Error(`unknown app ${w}`);

// 1. version (shared by both apps)
const cargoPath = join(root, "Cargo.toml");
let cargo = readFileSync(cargoPath, "utf8");
const current = cargo.match(/^version = "([^"]+)"/m)[1];
let version = current;
if (!process.argv.includes("--no-bump")) {
  const m = current.match(/^(\d+\.\d+\.\d+)-alpha\.(\d+)$/);
  if (!m) throw new Error(`not a preview version: ${current}`);
  version = `${m[1]}-alpha.${Number(m[2]) + 1}`;
  cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`);
  writeFileSync(cargoPath, cargo);
}
for (const a of Object.values(APPS)) {
  const conf = join(root, a.dir, "src-tauri/tauri.conf.json");
  writeFileSync(conf, readFileSync(conf, "utf8").replace(/"version": "[^"]+"/, `"version": "${version}"`));
}
console.log(`publishing ${which.join(" + ")} ${version}`);

const env = {
  ...process.env,
  TAURI_SIGNING_PRIVATE_KEY: readFileSync(join(keyDir, "ttv-tts-updater.key"), "utf8"),
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: readFileSync(join(keyDir, "ttv-tts-updater.password.txt"), "utf8"),
};
const bundle = join(root, "target/release/bundle/nsis");

for (const w of which) {
  const a = APPS[w];
  // 2. signed build
  sh("npx", ["tauri", "build", "--bundles", "nsis"], { cwd: join(root, a.dir), env });
  // 3. assets (no spaces in names: GitHub would rename them)
  const built = join(bundle, `${a.product}_${version}_x64-setup.exe`);
  const asset = `${a.asset}_${version}_x64-setup.exe`;
  copyFileSync(built, join(bundle, asset));
  const manifest = {
    version,
    notes: `Preview build ${version} - see https://github.com/${repo}/releases/tag/preview`,
    pub_date: new Date().toISOString(),
    platforms: { "windows-x86_64": { signature: readFileSync(`${built}.sig`, "utf8").trim(), url: `https://github.com/${repo}/releases/download/preview/${asset}` } },
  };
  writeFileSync(join(bundle, a.manifest), JSON.stringify(manifest, null, 2));
  // 4. installer first, manifest last (an updater never sees a manifest that
  // points at a missing file), then drop this app's older installers.
  sh("gh", ["release", "upload", "preview", "-R", repo, "--clobber", join(bundle, asset)]);
  sh("gh", ["release", "upload", "preview", "-R", repo, "--clobber", join(bundle, a.manifest)]);
  const assets = JSON.parse(out("gh", ["release", "view", "preview", "-R", repo, "--json", "assets"])).assets;
  const mine = (n) => n.startsWith(`${a.asset}_`) && n.endsWith("_x64-setup.exe");
  for (const x of assets) if (mine(x.name) && x.name !== asset) sh("gh", ["release", "delete-asset", "preview", x.name, "-R", repo, "--yes"]);
  console.log(`uploaded ${asset}`);
}

// 5. commit the version
if (version !== current) {
  sh("git", ["add", "Cargo.toml", "Cargo.lock", "apps/proxy/src-tauri/tauri.conf.json", "apps/app/src-tauri/tauri.conf.json"]);
  sh("git", ["commit", "-q", "-m", `Preview ${version}\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`]);
  sh("git", ["push", "-q"]);
}
console.log(`done: ${version}`);
