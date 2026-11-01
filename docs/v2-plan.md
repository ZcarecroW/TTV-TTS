# TTV-TTS 2.0 – plan

Status markers: ✅ done · 🔨 in progress · ⏳ planned

Decisions come from the interview of 2026-11-01 (see the project notes); this file tracks what is built.

## M1 – TTV-TTS Proxy ✅ (preview 2.0.0-alpha.1 … alpha.3)
- ✅ Workspace: `crates/vcp` (typed VCP client, WAV stream decoder), `crates/link` (protocol, discovery, pinning), `packages/ui` (design system, Nucleo icons)
- ✅ HTTPS with pinned self-signed certificate, pairing (6-digit code, per-device tokens), VCP-style key auth, rate limiting
- ✅ Unbuffered forwarding of VCP's API and web UI; own API under `/ttv/v1`; events WebSocket
- ✅ UDP discovery; VCP start/stop/restart/watchdog; automatic key takeover from VCP's settings
- ✅ System monitor incl. per-process GPU load/VRAM (Windows counters), NVIDIA temperature/power
- ✅ Firewall check + one-click fix + guides; log; signed self-update (stable/preview), remote update trigger
- ✅ UI EN/DE, dark/light, first-start assistant, tooltips; tested end-to-end on the real VCP PC

## M2 – Main app skeleton 🔨
- ⏳ Tauri app `apps/app`, shell with sidebar, settings, i18n EN/DE, theme
- ⏳ Voice host: discover proxies, pair (fingerprint shown), pinned TLS client; or VCP directly on the same PC
- ⏳ Proxy monitor in the app (VCP state, GPU, processes), Wake-on-LAN

## M3 – Chat to speech ⏳
- ⏳ Twitch IRC (anonymous, several channels), profiles per streamer
- ⏳ Text pipeline from 1.0: emotes (Twitch/BTTV/FFZ/7TV + 7TV live), emojis, links, mentions, repeats, caps, replacements, blocked words, name announcements
- ⏳ Language detection (1.0 detector + per-viewer memory)
- ⏳ Synthesis engine: sentence-split **sync generation with pipelining** (measured 7–8× faster than VCP streaming on the target PC), streaming as option, "Auto" benchmark, warm-up
- ⏳ Audio output to a chosen device (cpal), resampling, per-voice loudness matching
- ⏳ Queue, skip, pause, history/replay, approval mode, mod commands, timeouts/bans/deletions

## M4 – Presets & voices ⏳
- ⏳ 20 built-in presets × 240 language-neutral definitions, nested balanced tiers 30/60/120/240
- ⏳ "Classic 30" from the existing `TTS-xx NN Name (Role)` voices
- ⏳ Preset library UI, computed-voice status per language, listen with own text
- ⏳ Own presets: manual editor, AI prompt builder (chunks of 60), paste + validation (line format)
- ⏳ Viewer list: assigned voice, reassign, pin, mute; assignments stable when a preset grows

## M5 – Computing voices ⏳
- ⏳ Job engine (runs on the proxy, or in the app for single-PC setups): design → speech check → save → prime, in phases (fewer checkpoint swaps), resumable
- ⏳ ETA from the first voices, live progress; chat first, jobs in the gaps; free the design checkpoint afterwards
- ⏳ Best of 2–3 takes; long-job warnings
- ⏳ Lossless sample backup on the proxy, export/import of computed packs

## M6 – Extras ⏳
- ⏳ Emotions from chat cues (VCP emotional clone), translate-for-me (VCP /translate)
- ⏳ Built-in voice effects (DSP); VST3 + CLAP hosting
- ⏳ Global hotkeys, event announcements (optional, off by default), OS-voice fallback (optional)
- ⏳ Remote control API (Stream Deck, Streamer.bot), optional Twitch login (EventSub)

## M7 – Docs & release ⏳
- ⏳ Docs site EN + DE at zcarecrow.github.io/TTV-TTS with screenshots and voice samples
- ⏳ CI release workflow (Windows, macOS, Linux), update channel default "stable", 2.0.0
