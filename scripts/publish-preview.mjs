// Builds the next preview of TTV-TTS Proxy and publishes it to the rolling
// GitHub pre-release "preview", where installed proxies on the preview
// channel pick it up as an update.
//   node scripts/publish-preview.mjs            (bumps 2.0.0-alpha.N -> N+1)
//   node scripts/publish-preview.mjs --no-bump  (republish the current version)
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

// 1. version
const cargoPath = join(root, "Cargo.toml");
const confPath = join(root, "apps/proxy/src-tauri/tauri.conf.json");
let cargo = readFileSync(cargoPath, "utf8");
const current = cargo.match(/^version = "([^"]+)"/m)[1];
let version = current;
if (!process.argv.includes("--no-bump")) {
  const m = current.match(/^(\d+\.\d+\.\d+)-alpha\.(\d+)$/);
  if (!m) throw new Error(`not a preview version: ${current}`);
  version = `${m[1]}-alpha.${Number(m[2]) + 1}`;
  cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${version}"`);
  writeFileSync(cargoPath, cargo);
  writeFileSync(confPath, readFileSync(confPath, "utf8").replace(/"version": "[^"]+"/, `"version": "${version}"`));
}
console.log(`publishing TTV-TTS Proxy ${version}`);

// 2. signed build
const env = {
  ...process.env,
  TAURI_SIGNING_PRIVATE_KEY: readFileSync(join(keyDir, "ttv-tts-updater.key"), "utf8"),
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: readFileSync(join(keyDir, "ttv-tts-updater.password.txt"), "utf8"),
};
sh("npx", ["tauri", "build", "--bundles", "nsis"], { cwd: join(root, "apps/proxy"), env });

// 3. assets
const bundle = join(root, "target/release/bundle/nsis");
const built = join(bundle, `TTV-TTS Proxy_${version}_x64-setup.exe`);
const asset = `TTV-TTS-Proxy_${version}_x64-setup.exe`;
copyFileSync(built, join(bundle, asset));
const manifest = {
  version,
  notes: `Preview build of TTV-TTS Proxy ${version} - see https://github.com/${repo}/releases/tag/preview`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(`${built}.sig`, "utf8").trim(),
      url: `https://github.com/${repo}/releases/download/preview/${asset}`,
    },
  },
};
writeFileSync(join(bundle, "latest-proxy.json"), JSON.stringify(manifest, null, 2));

// 4. upload: installer first, manifest last (an updater never sees a manifest
// pointing at a file that isn't there yet), then drop older installers.
sh("gh", ["release", "upload", "preview", "-R", repo, "--clobber", join(bundle, asset)]);
sh("gh", ["release", "upload", "preview", "-R", repo, "--clobber", join(bundle, "latest-proxy.json")]);
const assets = JSON.parse(out("gh", ["release", "view", "preview", "-R", repo, "--json", "assets"])).assets;
for (const a of assets) {
  if (a.name.startsWith("TTV-TTS-Proxy_") && a.name !== asset) sh("gh", ["release", "delete-asset", "preview", a.name, "-R", repo, "--yes"]);
}

// 5. commit the version
if (version !== current) {
  sh("git", ["add", "Cargo.toml", "Cargo.lock", "apps/proxy/src-tauri/tauri.conf.json"]);
  sh("git", ["commit", "-q", "-m", `Proxy preview ${version}\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`]);
  sh("git", ["push", "-q"]);
}
console.log(`done: ${asset}`);
