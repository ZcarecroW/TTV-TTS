# Changelog

All notable changes to TTV-TTS are listed here. Downloads: [Releases](https://github.com/ZcarecroW/TTV-TTS/releases).

## Unreleased

- **Security review of everything that faces the internet**: a chat message could end TTV-TTS (and OBS links read on the voice host PC) – a “…” past the length limit, or a command written with look-alike letters. Fixed, and tests now throw hostile chat at every text path. Every server limits connections in total and per address, gives a request 20 seconds, closes idle connections and caps message sizes (before, a stranger could hold connections open for free); wrong remote-control tokens lock another PC out too. **New link** now also ends browser sources still connected with the old link (they kept getting the audio), and an OBS with an old link no longer locks its own PC out of the new one: only addresses from the internet are locked out for guessing. Invite codes, sound from the voice host PC and data from 7TV are checked strictly; the proxy loads plugins only from the plugin folders; LAN discovery answers only the local network. DuckDNS names work in any case and as sub-names (obs.name.duckdns.org).
- **HTTPS for links from the internet, automatic**: with a DuckDNS name and its token, TTV-TTS (and TTV-TTS Proxy) get a free certificate from Let's Encrypt for the name and renew it on their own – no port 80, no certificate files, nothing to know. OBS links from the internet then use `https://` (and an encrypted audio socket); the same port keeps plain HTTP for your own network. Once HTTPS works, a link from the internet over plain HTTP is refused, so its token never travels unencrypted. The DuckDNS name also gets updated for OBS links alone (every 5 minutes, so the daily reconnect is no problem), and TTV-TTS without a second PC updates it too.

- **System voices, managed**: Settings → System voice lists every installed voice per language (Windows: Microsoft’s voices and classic SAPI5 voices of other makers; macOS: “say”; Linux: espeak-ng), lets you pick one per language, set speed and pitch and try them. A language without a voice says so; on Windows one click installs Microsoft’s voices of that language (one admin prompt).
- **The router opens the ports itself (UPnP)**: TTV-TTS and TTV-TTS Proxy open exactly the ports of what you share on the internet (friends, OBS links), to their own PC, with a lease they renew, and close them again – no router settings. They tell when the connection has no public IPv4 address (DS-Lite, carrier NAT) and what a FRITZ!Box needs (“permit independent port sharing”). DuckDNS gets the router’s address, so a VPN on the PC no longer points the name at the VPN.
- **Audio plugins on the voice host PC**: with “Voice effects on the voice host PC”, TTV-TTS Proxy runs the VST3 and CLAP plugins it has installed too (matched by plugin id, so another folder or version works), with the settings made in TTV-TTS – also for OBS links read there. A plugin’s window can be opened on the voice host PC; its settings come back to TTV-TTS. Plugins it doesn’t have run on the gaming PC, with the whole chain of that message, to keep the order.
- **Plugin effects for viewers**: set up plugins with their own settings (for example a pitch shifter as “Chipmunk” and as “Giant”, with how often each is picked); a share of viewers per profile gets one at random and keeps it, like their voice. Not on top of voices that have an effect of their own (optional); changeable per viewer on the Viewers page. Each effect is one plugin instance shared by all viewers who have it.

## [2.1.0] – 2026-11-02

- **OBS links**: a profile can play its voices in OBS as a browser source instead of on a sound device – a source of its own with fader and filters. The page shows nothing and loads nothing but the voices (about 1 KB). Messages are only generated while OBS is connected.
- **OBS through the voice host PC**: TTV-TTS Proxy reads the profile's chat itself and sends the voices straight to OBS; the gaming PC does nothing for it and TTV-TTS may be closed. It reads only while OBS has the link open. The Live page shows it live, with pause, skip, clear and test messages; the proxy window lists the links.
- **Lend your voices to a friend**: an OBS link that also works from the internet (DuckDNS name, one forwarded port) – your friend pastes it into OBS, nothing to install. Links are 16 random characters; addresses that guess wrong are locked out.
- **Voice effects on the voice host PC** (default with TTV-TTS Proxy): the proxy applies the effect and loudness correction while generating; the gaming PC only plays.
- **Ready-made voices for several presets at once**, and all built-in presets are being computed in all ten languages for download.
- **The built-in Twitch login works out of the box** (the release carries its own Twitch application); using your own Twitch application is explained step by step.
- **Learn more** links to the documentation next to every complex setting, in the app and the proxy; a list of free VST3 and CLAP plugins with ideas for voices.
- Quality check: with many languages, a voice is compared with the median of its languages, so one odd version no longer flags the others. A voice whose takes only fail the speech check (whispers) keeps its best take instead of failing.
- The chat reader is shared by the app and the proxy (one code path); the app's installed frontend is half the size; debug build caches stay small.

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
