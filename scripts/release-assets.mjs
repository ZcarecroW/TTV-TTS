// Collects the bundles the release workflow built on Windows, macOS and Linux
// (downloaded into one folder), names them without spaces (GitHub would
// rename them) and writes the update manifests of both apps:
//   latest.json        TTV-TTS
//   latest-proxy.json  TTV-TTS Proxy
//   node scripts/release-assets.mjs <version> <downloaded dir> <out dir>
// Fails if an app misses an updatable build for a platform.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const [version, from, to] = process.argv.slice(2);
if (!version || !from || !to) throw new Error("usage: release-assets.mjs <version> <downloaded dir> <out dir>");
const repo = process.env.GITHUB_REPOSITORY ?? "ZcarecroW/TTV-TTS";
const tag = `v${version}`;
mkdirSync(to, { recursive: true });

const files = [];
(function walk(d) {
  for (const n of readdirSync(d)) {
    const p = join(d, n);
    // a macOS .app is a folder: only its tarball is an asset
    if (statSync(p).isDirectory()) {
      if (!n.endsWith(".app")) walk(p);
    } else files.push(p);
  }
})(from);
const find = (name) => files.find((f) => basename(f) === name);

const APPS = [
  { product: "TTV-TTS", asset: "TTV-TTS", manifest: "latest.json" },
  { product: "TTV-TTS Proxy", asset: "TTV-TTS-Proxy", manifest: "latest-proxy.json" },
];
const urlOf = (name) => `https://github.com/${repo}/releases/download/${tag}/${name}`;
let missing = 0;

for (const a of APPS) {
  const p = a.product;
  const v = version;
  // [built file name, asset name, updater platforms, required]. A .deb or
  // .rpm install looks for its own key (linux-x86_64-deb/-rpm) before the
  // AppImage's; those are added when their signature exists.
  const parts = [
    [`${p}_${v}_x64-setup.exe`, `${a.asset}_${v}_x64-setup.exe`, ["windows-x86_64"], true],
    [`${p}.app.tar.gz`, `${a.asset}_${v}_universal.app.tar.gz`, ["darwin-x86_64", "darwin-aarch64"], true],
    [`${p}_${v}_universal.dmg`, `${a.asset}_${v}_universal.dmg`, [], false],
    [`${p}_${v}_amd64.AppImage`, `${a.asset}_${v}_amd64.AppImage`, ["linux-x86_64"], true],
    [`${p}_${v}_amd64.deb`, `${a.asset}_${v}_amd64.deb`, ["linux-x86_64-deb"], false],
    [`${p}-${v}-1.x86_64.rpm`, `${a.asset}-${v}-1.x86_64.rpm`, ["linux-x86_64-rpm"], false],
  ];
  const platforms = {};
  for (const [built, asset, keys, required] of parts) {
    const f = find(built);
    if (!f) {
      console.log(`::warning::${p}: ${built} wasn't built`);
      if (required) missing++;
      continue;
    }
    copyFileSync(f, join(to, asset));
    if (!keys.length) continue;
    if (!existsSync(`${f}.sig`)) {
      if (required) {
        console.log(`::error::${p}: ${built}.sig is missing (signing key not set?)`);
        missing++;
      }
      continue;
    }
    const signature = readFileSync(`${f}.sig`, "utf8").trim();
    for (const k of keys) platforms[k] = { signature, url: urlOf(asset) };
  }
  const manifest = {
    version,
    notes: `TTV-TTS ${version} – what's new: https://github.com/${repo}/releases/tag/${tag}`,
    pub_date: new Date().toISOString(),
    platforms,
  };
  writeFileSync(join(to, a.manifest), JSON.stringify(manifest, null, 2));
  console.log(`${a.manifest}: ${Object.keys(platforms).join(", ")}`);
}
console.log(readdirSync(to).join("\n"));
if (missing) {
  console.log(`::error::${missing} updatable builds are missing`);
  process.exit(1);
}
