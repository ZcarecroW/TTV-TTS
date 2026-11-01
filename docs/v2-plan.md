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

## M4 – Presets & voices ✅
- ✅ 20 built-in presets × 240 language-neutral definitions (4,800 unique), nested balanced tiers 30/60/120/240 – checked by a test; parse 3.6 ms, preset list over 48,000 computed voices 3.4 ms
- ✅ "Classic" adoption of the existing `TTS-xx NN Name (Role)` voices (definitions rebuilt from VCP's design text, nothing regenerated)
- ✅ Preset library UI, computed-voice status per language, listen with own text; profile voice mode "preset" (stable roles, fallback language)
- ✅ Own presets: manual editor, AI prompt builder (parts of 60), paste + validation (line format), export as text
- ✅ Viewer list per profile: assigned voice, reassign by preset voice name, mute, forget; pinned voices shown; assignments stable when a preset grows

## M5 – Computing voices 🔨 (first real run 2026-11-01: Cinematic 60 × de+en on the VCP PC, ~22 s per voice)
- ✅ Job engine (`crates/engine`, runs on the proxy, or in the app for single-PC setups): design → speech check → save → prime, in phases, resumable; library as append-only log with incremental change events
- ✅ ETA (benchmark, then measured), live progress, pause/resume/cancel; chat first, jobs in the gaps; every batch ends with priming, so the speaking checkpoint is loaded again for chat; a batch where VCP designs nothing is retried later instead of failing voices
- ✅ Best of 1–3 takes; long-job warnings
- ✅ Voice design names gender and age first ("A 35-year-old man. …"): fixed the pitch of 3 of 4 off voices in a test on the VCP PC
- ✅ Hardened after the first real run: a take VCP could not transcribe is never saved unchecked (broken speech recognition retries the batch); every new voice is primed until VCP reports its clone prompt (the first voice after a checkpoint swap lost it); a voice VCP can't speak with is deleted again; failed batches back off up to 10 min; progress inside a batch (phase step/steps); "Check voices" removes voices deleted in VCP and re-primes voices without clone prompt; engine tests against a fake VCP (`crates/engine/tests/fake_vcp.rs`)
- ✅ Lossless sample backup (FLAC) on the computing PC
- ✅ Quality check (optional per job): loudness, YIN pitch vs gender/age (description words allow intended traits), pauses, pace (per-language rates measured on 420 real voices), noise, clipping, how the voice speaks a new sentence (ASR), languages compared; review list with listening (new sentence / original sample), keep, design again, improve (replace only if better), check existing voices; automatic improvement; take selection by plausibility. Real run: same 33 of 120 Cinematic voices flagged as offline
- ✅ Classic adoption builds descriptions/gender/age from the 1.x role names; old adoptions are migrated (re-keyed, nothing changes in VCP)
- ✅ Voice packs (.ttvpack: zip with manifest + FLAC): share a preset's voices or selected ones, import packs (checked, limits); live round trip on the real VCP
- ✅ Ready-made voices: catalog + packs on the release "voice-packs", download + SHA-256 + import in the app; `scripts/build-voice-packs.mjs` builds them (first wave: 30 voices × de+en of all 20 presets, computing on the VCP PC)
- ✅ Friends over the internet: guest API on its own port, one-time invites with pinned certificate, quotas, host first, DuckDNS, reachability test, guide (FRITZ!Box, DS-Lite, Tailscale), join with a code, guest mode; e2e security script
- ⏳ Proxy window page for internet access (the main app manages it today)
- ✅ Update the proxy from the app (check + install now)

## M6 – Extras ⏳
- ✅ Emotions from chat cues (VCP emotional clone; emotes, emojis, words, shouting; capped per profile)
- ✅ Translate for me: MyMemory (free, ~0.5 s), DeepL (own key) or VCP (offline; ~20 s because VCP holds one model); read the translation, both, or show it
- ✅ Built-in voice effects (radio, telephone, megaphone, robot, cave, hall, walkie-talkie, underwater, whisper): per preset voice, per viewer in the viewer list, switch per profile; loudness matched on 60 real voices, < 0.1 % of real time
- ⏳ VST3 + CLAP hosting
- ⏳ Global hotkeys, event announcements (optional, off by default), OS-voice fallback (optional)
- ⏳ Remote control API (Stream Deck, Streamer.bot), optional Twitch login (EventSub)

## M7 – Docs & release ⏳
- ⏳ Docs site EN + DE at zcarecrow.github.io/TTV-TTS with screenshots and voice samples
- ⏳ CI release workflow (Windows, macOS, Linux), update channel default "stable", 2.0.0
