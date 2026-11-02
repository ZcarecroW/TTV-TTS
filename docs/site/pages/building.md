---
title: Building from source
description: For developers – the project layout, building and testing TTV-TTS and the proxy.
icon: Console
---

TTV-TTS is open source under the MIT license: [github.com/ZcarecroW/TTV-TTS](https://github.com/ZcarecroW/TTV-TTS).

## Stack {#stack}

- [Tauri 2](https://tauri.app) with a **Rust** core, **Vue 3** + TypeScript + Tailwind for the interface, built with Vite.
- One Cargo workspace and one npm workspace:

| Path | |
|---|---|
| `apps/app` | TTV-TTS (Vue frontend, `src-tauri` Rust core) |
| `apps/proxy` | TTV-TTS Proxy |
| `packages/ui` | shared design system: theme, components, icons, shared texts |
| `crates/vcp` | Voice Creator Pro API client, speed monitor |
| `crates/link` | the app ↔ proxy protocol |
| `crates/engine` | computing presets: design, check, save, quality check, packs |
| `crates/plugins` | VST3 and CLAP hosting |
| `docs/site` | this documentation |
| `legacy/v1` | TTV-TTS 1.x (served unchanged on the website) |

## Requirements {#requirements}

- Rust (stable) and Node.js (current LTS).
- Windows: Microsoft C++ Build Tools and WebView2 (preinstalled on Windows 11).
- Linux: `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev`.

## Develop {#develop}

```sh
npm ci
npm run tauri --workspace @ttv-tts/app -- dev      # TTV-TTS
npm run proxy:dev                                  # TTV-TTS Proxy
npm run dev --workspace @ttv-tts/app               # the interface in a browser, with a demo backend
```

The browser mode needs no Rust and no Voice Creator Pro: a demo backend answers, which the UI checks and the documentation's screenshots use too.

## Test {#test}

```sh
npm run typecheck
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node apps/app/scripts/ui-check.mjs                 # UI check in Edge (dev server on :1440)
node apps/proxy/scripts/ui-check.mjs               # (dev server on :1430)
```

Tests that need a real Voice Creator Pro are marked `#[ignore]`; their comments say which environment variables they need.

## Build {#build}

```sh
npx tauri build          # in apps/app or apps/proxy
```

Releases are built by GitHub Actions for Windows, macOS and Linux when a version tag is pushed. Without the `TTV_TWITCH_CLIENT_ID` variable a build has no Twitch application – the optional login then asks for your own client ID.

## Icons {#icons}

The icons are from [Nucleo](https://nucleoapp.com), licensed to the author for TTV-TTS. They are part of the apps and this website but not covered by the MIT license: don't extract or reuse them.

## Documentation {#docs}

```sh
node docs/site/build.mjs --serve      # build and preview this site
node docs/site/shots.mjs              # screenshots from the demo mode (Edge)
```
