---
title: Aus dem Quellcode bauen
description: Für Entwickler – der Aufbau des Projekts, TTV-TTS und den Proxy bauen und testen.
icon: Console
---

TTV-TTS ist Open Source unter der MIT-Lizenz: [github.com/ZcarecroW/TTV-TTS](https://github.com/ZcarecroW/TTV-TTS).

## Technik {#stack}

- [Tauri 2](https://tauri.app) mit einem Kern in **Rust**, **Vue 3** + TypeScript + Tailwind für die Oberfläche, gebaut mit Vite.
- Ein Cargo-Workspace und ein npm-Workspace:

| Pfad | |
|---|---|
| `apps/app` | TTV-TTS (Vue-Oberfläche, Rust-Kern in `src-tauri`) |
| `apps/proxy` | TTV-TTS Proxy |
| `packages/ui` | gemeinsames Design-System: Theme, Komponenten, Icons, gemeinsame Texte |
| `crates/vcp` | Client für die API von Voice Creator Pro, Tempo-Messung |
| `crates/link` | das Protokoll zwischen App und Proxy |
| `crates/engine` | Presets berechnen: entwerfen, prüfen, speichern, Qualitätsprüfung, Pakete |
| `crates/plugins` | VST3- und CLAP-Hosting |
| `docs/site` | diese Dokumentation |
| `legacy/v1` | TTV-TTS 1.x (unverändert auf der Website) |

## Voraussetzungen {#requirements}

- Rust (stable) und Node.js (aktuelles LTS).
- Windows: Microsoft C++ Build Tools und WebView2 (unter Windows 11 vorinstalliert).
- Linux: `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev`.

## Entwickeln {#develop}

```sh
npm ci
npm run tauri --workspace @ttv-tts/app -- dev      # TTV-TTS
npm run proxy:dev                                  # TTV-TTS Proxy
npm run dev --workspace @ttv-tts/app               # die Oberfläche im Browser, mit Demo-Backend
```

Der Browser-Modus braucht weder Rust noch Voice Creator Pro: Ein Demo-Backend antwortet – dasselbe nutzen auch die UI-Prüfungen und die Screenshots dieser Doku.

## Testen {#test}

```sh
npm run typecheck
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node apps/app/scripts/ui-check.mjs                 # UI-Prüfung in Edge (Dev-Server auf :1440)
node apps/proxy/scripts/ui-check.mjs               # (Dev-Server auf :1430)
```

Tests, die ein echtes Voice Creator Pro brauchen, sind mit `#[ignore]` markiert; ihre Kommentare nennen die nötigen Umgebungsvariablen.

## Bauen {#build}

```sh
npx tauri build          # in apps/app oder apps/proxy
```

Fertige Versionen baut GitHub Actions für Windows, macOS und Linux, sobald ein Versions-Tag hochgeladen wird. Ohne die Variable `TTV_TWITCH_CLIENT_ID` hat ein Build keine Twitch-Anwendung – die optionale Anmeldung fragt dann nach deiner eigenen Client-ID.

## Icons {#icons}

Die Icons sind von [Nucleo](https://nucleoapp.com), für den Autor für TTV-TTS lizenziert. Sie sind Teil der Apps und dieser Website, aber nicht von der MIT-Lizenz abgedeckt: Bitte nicht herauslösen oder anderweitig verwenden.

## Dokumentation {#docs}

```sh
node docs/site/build.mjs --serve      # diese Website bauen und ansehen
node docs/site/shots.mjs              # Screenshots aus dem Demo-Modus (Edge)
```
