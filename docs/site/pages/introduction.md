---
title: Introduction
description: What TTV-TTS does, how it works and what you need.
icon: Sparkle
---

**TTV-TTS** reads your Twitch chat aloud with natural AI voices. The voices come from **[Voice Creator Pro](https://voicecreator.pro)** (VCP), which runs on your own graphics card – on your streaming PC or on a second computer. No cloud service, no subscription, no cost per message, and no Twitch login needed.

![The Live page: what is being read, what comes next, what was read](live.webp "The Live page")

## What makes it different

- **Every viewer has their own voice** and keeps it – in every language. A regular sounds the same today and next month.
- **Presets** are casts of characters: 20 built in (Fantasy Tavern, Starship Crew, Noir City …), each with 240 distinct voices. Voice Creator Pro designs them once; after that they are ready instantly. You can also [write your own](presets#own-presets).
- **Any language**: each message is read in its own language – or [translated for you](profiles#translate) if you don't speak it.
- **Feelings and effects**: laughing emotes make a voice laugh, crying emojis make it sad; a few voices can sound like a radio, a robot or a cave, and you can add your own [VST3 and CLAP plugins](effects#plugins).
- **You stay in control**: filters, an approval queue, skip and replay, global hotkeys, mod commands in chat and a [remote control API](remote-api) for Stream Deck and Streamer.bot.
- **Two PCs, no problem**: the small [TTV-TTS Proxy](proxy) runs next to Voice Creator Pro on another PC, pairs with a code and keeps the connection encrypted.

## How it works {#how-it-works}

1. TTV-TTS connects to Twitch chat **anonymously** (read-only, like a viewer who isn't logged in).
2. Each message is cleaned up: emotes, links, spam and names are handled the way you set it in the [profile](profiles).
3. TTV-TTS picks the viewer's voice and the message's language, and asks Voice Creator Pro to speak – directly on this PC, or through the proxy on the other one.
4. The audio comes back and plays on the output device you chose. OBS picks it up like any other sound.

While chat is quiet, Voice Creator Pro computes the voices of your presets in the background. When a message arrives, chat goes first.

## What you need {#requirements}

| | |
|---|---|
| **Voice Creator Pro** | 2.4 or newer, on Windows or macOS, with its **HTTP API** switched on in its settings (port 8100). |
| **Graphics card** | One that runs Voice Creator Pro well. Measured on an RTX 3070: a second of speech takes about half a second. |
| **Streaming PC** | Windows 10/11, macOS 11+ or Linux for TTV-TTS itself. It needs little: the voices are made on the voice host. |
| **Network** | Only if VCP runs on another PC: both in the same home network (or see [friends over the internet](friends)). |

> [!TIP]
> Coming from TTV-TTS 1.x, the OBS browser source? Your old voices are adopted automatically – see [Coming from 1.x](from-v1).

## Next steps

- [Install TTV-TTS](installation) (and the proxy, if VCP runs on another PC).
- [Connect it to Voice Creator Pro](voice-host).
- [Get ready for your first stream](first-stream).
