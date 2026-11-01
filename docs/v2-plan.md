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

## M2 – Main app skeleton 🔨 (slice A done: host + audio + speech test)
- ✅ Tauri app `apps/app`, shell with sidebar, settings, i18n EN/DE, theme
- ✅ Voice host: discover proxies, pair (fingerprint shown), pinned TLS client; or VCP directly on the same PC
- ✅ Proxy monitor in the app (VCP state, GPU, processes), Wake-on-LAN
- ✅ Audio output (cpal 0.18, rubato FFT resampling, gapless queue, fade on skip), sentence-pipelined sync generation: first audio 1.4 s, RTF 0.65 through the real proxy

## M3 – Chat to speech 🔨 (slice B done: chat → speech works end to end)
- ✅ Twitch IRC (anonymous, several channels on one connection, reconnect), profiles per streamer (auto-saved editor)
- ✅ Text pipeline from 1.0: emotes (Twitch/BTTV/FFZ/7TV), emojis, links, mentions, repeats, caps, replacements, blocked words, name announcements (7TV live updates still ⏳)
- ✅ Language detection (1.0 detector + per-viewer memory)
- ✅ Synthesis: sentence-split **sync generation with pipelining** (measured 7–8× faster than VCP streaming on the target PC); streaming option + "Auto" benchmark + warm-up ⏳
- ✅ Audio output to a chosen device (cpal), resampling, per-voice loudness matching (gain measured when a voice is computed)
- ✅ Queue, skip, pause, clear, history with reasons for filtered messages, mod commands, timeouts/bans/deletions (replay + approval mode ⏳)

## M4 – Presets & voices 🔨
- ✅ 20 built-in presets × 240 language-neutral definitions (4,800 unique), nested balanced tiers 30/60/120/240 – checked by a test; parse 3.6 ms, preset list over 48,000 computed voices 3.4 ms
- ✅ "Classic" adoption of the existing `TTS-xx NN Name (Role)` voices (definitions rebuilt from VCP's design text, nothing regenerated)
- ✅ Preset library UI, computed-voice status per language, listen with own text; profile voice mode "preset" (stable roles, fallback language)
- ✅ Own presets: manual editor, AI prompt builder (parts of 60), paste + validation (line format), export as text
- ⏳ Viewer list: assigned voice, reassign, pin, mute; assignments stable when a preset grows

## M5 – Computing voices 🔨 (needs a test run on the real VCP PC)
- ✅ Job engine (`crates/engine`, runs on the proxy, or in the app for single-PC setups): design → speech check → save → prime, in phases, resumable; library as append-only log with incremental change events
- ✅ ETA (benchmark, then measured), live progress, pause/resume/cancel; chat first, jobs in the gaps; free the design checkpoint afterwards ⏳
- ✅ Best of 1–3 takes; long-job warnings
- ✅ Lossless sample backup (FLAC) on the computing PC; export/import of computed packs ⏳
- ✅ Update the proxy from the app (check + install now)

## M6 – Extras ⏳
- ⏳ Emotions from chat cues (VCP emotional clone), translate-for-me (VCP /translate)
- ⏳ Built-in voice effects (DSP); VST3 + CLAP hosting
- ⏳ Global hotkeys, event announcements (optional, off by default), OS-voice fallback (optional)
- ⏳ Remote control API (Stream Deck, Streamer.bot), optional Twitch login (EventSub)

## M7 – Docs & release ⏳
- ⏳ Docs site EN + DE at zcarecrow.github.io/TTV-TTS with screenshots and voice samples
- ⏳ CI release workflow (Windows, macOS, Linux), update channel default "stable", 2.0.0
