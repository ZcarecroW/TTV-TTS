---
title: FAQ
description: Short answers to common questions.
icon: CircleInfo
---

### Do I need a Twitch login? {#login}

No. TTV-TTS reads chat anonymously, like a viewer who isn't logged in. Log in only if follows and channel-point rewards without text should be [announced](profiles#announcements).

### What does it cost? {#cost}

TTV-TTS and the proxy are free and open source (MIT). You need [Voice Creator Pro](https://voicecreator.pro) and a graphics card that runs it. Nothing is billed per message; the optional translation by MyMemory is free, DeepL with your own key has a free tier.

### How fast is it? {#speed}

Measured on an RTX 3070 with Qwen3 in Voice Creator Pro: a message starts about 1.3 to 1.8 seconds after it appears in chat, and one second of speech takes about half a second to generate. The first message after a voice was designed takes longer (about 18 s), because Voice Creator Pro has to load its speaking model again – see [Troubleshooting](troubleshooting#slow-first).

### Does it slow down my game? {#game}

On one PC, Voice Creator Pro shares the graphics card with your game while it speaks or computes. That's why many streamers run Voice Creator Pro on a second PC with the [proxy](proxy) – TTV-TTS itself needs little.

### Can viewers abuse it? {#abuse}

You decide what is read: roles, a command or a channel-points reward, limits per message and per viewer, blocked words, link handling, an [approval queue](live#approval), mod commands to skip and mute, and messages deleted by mods are never read.

### Which languages? {#languages}

As many as your Voice Creator Pro model speaks – Qwen3 speaks English, German, French, Spanish, Italian, Portuguese, Russian, Japanese, Chinese and Korean. Presets are computed per language; TTV-TTS also handles languages it has no computed voices for with the fallback language.

### Can I use my own voices? {#own-voices}

Yes: write your [own presets](presets#own-presets) (with an AI or by hand), pin any Voice Creator Pro voice to a viewer, or use the voice modes *voice per viewer* / *one voice* with your own voices.

### Does it work with OBS? {#obs}

Yes – TTV-TTS plays on an audio device, which OBS records as desktop audio or with *Application Audio Capture*. See [Into OBS](first-stream#obs).

### Is the old OBS browser source still there? {#v1}

Yes, TTV-TTS 1.x keeps working at its old address, but it isn't developed further. See [Coming from 1.x](from-v1).

### Where are my voices stored? {#storage}

In Voice Creator Pro. TTV-TTS keeps a library of which voice belongs to which preset character and language, plus (optionally) lossless backups of the samples.
