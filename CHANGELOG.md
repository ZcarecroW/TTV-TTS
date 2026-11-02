# Changelog

All notable changes to TTV-TTS are listed here. Downloads: [Releases](https://github.com/ZcarecroW/TTV-TTS/releases).

## [2.0.0] – 2026-11-02

TTV-TTS is now a desktop app for Windows, macOS and Linux – plus the TTV-TTS Proxy for a second PC with Voice Creator Pro. The OBS browser source of 1.x keeps working at its old address.

- **Desktop app** (Tauri, Rust, Vue): plays on an audio device that OBS records; dark blue or light theme; English and German; explanations next to every setting; signed auto-updates (stable and preview channels).
- **TTV-TTS Proxy** for the Voice Creator Pro PC: pairing with a six-digit code, encrypted with its own pinned certificate, found automatically in the network; firewall check and one-click fix (Windows Firewall, Windows Firewall Control, security suites); starts, watches and restarts Voice Creator Pro; Wake-on-LAN; live hardware monitor; tray app.
- **Presets**: 20 built-in casts with 240 voices each, computed per language by Voice Creator Pro in pauses between chat messages (chat always goes first), with takes, speech check, lossless backups, a quality check that finds odd voices and improves them, "design again" with a choice of three takes, time estimates measured on your voice host; own presets in a line format, written by hand or with any AI chat.
- **Every viewer keeps a voice** of the preset in every language; change it, mute or give one viewer an effect on the Viewers page.
- **Voice packs** to share computed voices, and ready-made downloads for the built-in presets.
- **Your 1.x voices** are adopted as the preset *Classic 30*.
- **Emotions from chat** (emotes, emojis, words, shouting), **translate for me** (MyMemory, DeepL or Voice Creator Pro), **voice effects** (radio, telephone, megaphone, robot, cave, hall, walkie-talkie, underwater, whisper) and **VST3/CLAP plugins**.
- **Control**: approval queue, skip, pause, replay of the last messages, global hotkeys, mod commands in chat, a **remote control API** for Stream Deck and Streamer.bot, a **system voice fallback** when Voice Creator Pro is down.
- **Announcements** (off by default) of subs, gifts, raids and cheers – and follows and channel-point rewards without text with the optional **Twitch login** (device code, EventSub).
- **Speed monitor**: TTV-TTS measures how fast Voice Creator Pro speaks; the proxy restarts it at a quiet moment when it gets clearly slower.
- **Friends over the internet**: lend your computed voices to friends through a separate, limited port, with invites, daily limits and DuckDNS.
- 7TV emote changes arrive live; BetterTTV, FrankerFaceZ and Twitch emotes as before.
- Documentation in English and German with screenshots and voice samples: https://zcarecrow.github.io/TTV-TTS/

## [1.0.0] – 2026-10-18

First public release.

- **ttv-tts.html**: OBS browser source that reads Twitch chat anonymously and speaks it through the Voice Creator Pro REST API.
  - Streaming Web Audio playback at 24 kHz (starts before VCP has finished generating), prefetch of the next message, and a warm-up after VCP restarts.
  - Language detection for every message, with per-viewer memory. Each viewer gets a fixed voice role in every language (`voices.mode: 'perUserLanguage'`).
  - Twitch, BTTV, FFZ and 7TV emote handling. Emojis are turned into words (de/en/ru). Links, mentions, letter spam and ALL CAPS are cleaned up.
  - Name announcements when the speaker changes, mod commands, and automatic removal of messages after timeouts, bans and deletions.
  - New voices are primed automatically (fixes "Generation failed"), plus a debug panel and console helpers.
- **connection-check.html**: step-by-step test of the whole chain, with a speech test and timing.
- **voice-lab.html**: creates and checks a matching set of role voices (design → speech check → save → prime → verify).
- **vcp-lan-setup.ps1**: makes VCP reachable from a second PC (port proxy, firewall rule, autostart check, status report).
- **readme.html**: full guide and configuration reference.

[1.0.0]: https://github.com/ZcarecroW/ttv-tts/releases/tag/v1.0.0
