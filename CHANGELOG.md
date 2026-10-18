# Changelog

All notable changes to TTV-TTS are listed here. Downloads: [Releases](https://github.com/ZcarecroW/ttv-tts/releases).

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
