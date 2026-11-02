---
title: Your first stream
description: From the first start to chat being read aloud – in about ten minutes.
icon: Rocket
---

After [installing](installation) and [connecting Voice Creator Pro](voice-host), four things are left.

## 1. Your channel {#channel}

Open **Profiles**. There is one profile already. Enter your Twitch channel name under **Channels** and press Enter. A profile can read several channels – useful for collaborations.

Then switch the profile on: on the **Live** page, under **Reading now**, turn on the switch next to it. TTV-TTS joins the chat; **Chat connected** appears.

## 2. Voices {#voices}

On **Presets**, choose a cast of voices – for example *Fantasy Tavern* – and click **Compute**. Pick:

- **How many voices**: 30 are plenty for most channels; 60, 120 or 240 for big chats.
- **Languages**: the languages your chat writes in. Each language is computed separately.

TTV-TTS shows how long it will take, measured on your voice host – roughly half a minute per voice. You can stream meanwhile: chat always goes first.

![Computing a preset: voices, languages and the estimated time](preset-compute.webp "Compute")

> [!TIP]
> Don't want to wait? Many presets can be **downloaded ready-made** (button **Get**): a few megabytes, about a minute to import, no computing.

Then, in the profile under **Voices**, choose that preset. Viewers get one of its voices automatically when they first write.

## 3. Hear it {#test}

On the **Live** page, type something under **Test message** and click **Send**. It goes the same way as a real chat message: cleaned up, a voice chosen, read aloud.

## 4. Into OBS {#obs}

The easiest way: an **[OBS link](obs)**. Set the profile's **Where the voices play** to **OBS on this PC** (or **OBS through the voice host PC**), copy the link and add it in OBS as a *Browser* source with **Control audio via OBS** ticked. The voices are then a source of their own in OBS's mixer.

Or TTV-TTS plays on the output device under **Settings → Sound** (normally your system's default), and OBS hears it in one of two ways:

- **Desktop audio** – if OBS records your default output device, TTV-TTS is in the stream already.
- **Application Audio Capture** (OBS 28+, Windows 10 2004+): add a source *Application Audio Capture* and choose TTV-TTS. Then you can mix its volume separately.

To keep TTV-TTS out of your headphones but in the stream, choose a virtual cable (VB-Cable, VoiceMeeter) as the output device in TTV-TTS and add it in OBS.

## While you stream {#live}

- **Skip** stops the current message, **Pause** holds new ones, **Clear queue** drops what's waiting – or use [hotkeys](settings#hotkeys) while you're in a game.
- Your mods can use chat commands like `!skip` and `!ttspause` (see [Profiles](profiles#mod-commands)).
- A viewer sounds wrong? Change their voice on the [Viewers](viewers) page.

That's it – go live. The next pages explain every setting in detail.
