---
title: Profiles
description: Per streamer or per channel – what is read, how it is cleaned up and which voices speak.
icon: Users
---

A **profile** holds everything about reading one streamer's chat: channels, rules, voices, languages. Most streamers need one; make more for different channels or shows (**New profile**). Changes are saved automatically.

![The Profiles page](profiles.webp)

## Profile and channels {#basics}

**Channels** – one or more Twitch channel names. TTV-TTS reads them anonymously; no login needed. Switch the profile on on the [Live](live) page.

## What is read {#reading}

- **Read** – *every message*, *only “!tts …”* (with your own command), or *only these roles* (broadcaster, mods, VIPs, subs).
- **Only this channel-points reward** – paste a reward's ID to read only messages that come with that reward (a reward with text, for example “Text to speech”).
- **Ignore the broadcaster's own messages**, **Ignored users** (bots like Nightbot), **Ignore messages starting with** (`!` for commands).
- **Read texts attached to subs and resubs** – the message a viewer writes with their resub.
- **Approve messages before they are read** – see [Approving messages](live#approval).
- **System voice when the voice host is down** – see [System voice fallback](profiles#os-voice).

### System voice fallback {#os-voice}

With **System voice when the voice host is down**, messages aren't lost when Voice Creator Pro can't speak (stopped, restarting, its PC off or unreachable): your operating system's own voice reads them instead – plain, but chat keeps going. Which voice reads which language, how fast and how high is set in [Settings → System voice](settings#system-voice) – also how to install more voices. As soon as Voice Creator Pro answers again, its voices take over.

### Limits {#limits}

**Max. characters**, **Max. seconds per message**, **Cooldown per viewer** and **Max. waiting messages** keep a busy chat from piling up.

### Names {#names}

**Say who writes** – *on change* (only when someone else writes), *always* or *never*. **Clean up names** turns `max_123` into “max”. Under **Pronunciation of names** you can write how a name should be spoken.

## Language {#language}

TTV-TTS detects the language of each message and reads it in that language with the viewer's voice. **Default language** is used when a message is too short to tell. **Remember a viewer's language** keeps a viewer's language for a few minutes, so a short “lol” after a German sentence is still read in German. **Languages the model doesn't speak** decides what happens with messages in a language your Voice Creator Pro model can't say.

## Translate for me {#translate}

If your chat writes in languages you don't speak, switch on **Read other languages in mine**:

![Translate for me](profiles-translate.webp)

- **Into** – your language (the profile's language by default).
- **Translated by** – *MyMemory* (free, online), *DeepL* (your own API key, see [Settings](settings#translation)) or *Voice Creator Pro* (offline, but slower).
- **Languages I understand** – never translated.
- **Read** – *the translation*, *original, then translation*, or *original, show translation* (the translation only appears on the Live page).

The viewer keeps their voice – it just speaks your language. Type something under **Try** to test it.

## Emotes, emojis and text {#cleanup}

- **Emotes** – Twitch, BetterTTV, FrankerFaceZ and 7TV emotes (7TV changes arrive live): *each once*, *remove* or *read all*; **Split names** reads `PepeLaugh` as “Pepe Laugh”; **Pronunciation of emotes** sets how one is spoken.
- **Emojis** – *as words* (“😂” → “laughing”), *remove* or *keep*.
- **Links** – say “link”, remove them, or skip the message. **@mentions** – read the name or remove it.
- **Shorten repeats** (`sooooo` → “so”), **Lower-case ALL CAPS**.
- **Blocked words** – skip the message or say “beep”. **Replacements** – your own word list, per language.

## Voices {#voices}

**Who gets which voice**:

| Mode | |
|---|---|
| **preset** | every viewer gets a voice of the chosen [preset](presets), the same in every language (recommended) |
| **role per viewer** | for voice libraries named like `TTS-de 03 …`, `TTS-en 03 …` – each viewer keeps a role, spoken by that role's voice of the message's language (the 1.x scheme) |
| **voice per viewer** | one voice from all of Voice Creator Pro's voices per viewer |
| **voice per language** / **one voice** | simple setups |

With *preset* you choose the preset and how many of its voices are used (*all*, or the first 30, 60 …). **Default voice** speaks when no preset voice is available yet. **Fixed voices for viewers** pins a voice to someone; **Never use these voices** leaves voices out.

### Emotions from chat {#emotions}

With **Emotions from chat** on, laughing emotes (KEKW), emojis (😂 😭 😱), words like “haha”, “rip” or “nooo” and shouting make the voice sound happy, sad, scared or surprised. **At most** limits how strong it gets. Mixed or unclear messages stay neutral. It needs a model with emotions in Voice Creator Pro (Qwen3) and doesn't make speaking slower.

### Voice effects {#effects}

**Voice effects** lets the effects of a preset play (radio, telephone …) – see [Effects & plugins](effects).

### Plugin effects for viewers {#viewer-plugins}

**Plugin effects for viewers: … % get one** – the share of viewers who get one of the [plugin effects](effects#viewer-plugins) from the settings at random. **Not on voices that have an effect of their own** keeps a radio voice from getting a pitch shifter on top.

## Announcements {#announcements}

Off by default – chat bots often announce already. When on, TTV-TTS announces events with an announcer voice of your choice:

![Announcements](profiles-events.webp)

- Subs and resubs, gifted subs (20 gifts at once are one announcement), raids (from a number of viewers), cheers (from a number of bits) – all without a Twitch login.
- **Follows** and **Channel-point rewards without text** need the optional [Twitch login](settings#twitch). Follows are off by default (follow bots), and at most one follow is announced every 10 seconds.
- **Own sentences** with placeholders like `{user}`, `{months}`, `{viewers}`, `{bits}`, `{reward}`; ▶ plays an example.

## Mod commands {#mod-commands}

With **Mods may control TTS in chat**, these commands work for the roles you allow (broadcaster and mods by default):

| Command | |
|---|---|
| `!skip` | skip the current message |
| `!ttsclear` | clear the queue |
| `!ttspause` / `!ttsresume` | pause / resume |
| `!ttsvol 60` | volume in percent |
| `!ttsmute name` / `!ttsunmute name` | mute / unmute a viewer |

All of them can be renamed. **Timeouts, bans and deleted messages leave the queue** – a message a mod deletes is never read.
