---
title: Live
description: What is being read, what comes next – and how to step in.
icon: Waveform
---

The **Live** page is where you watch TTV-TTS work during a stream.

![The Live page](live.webp)

## Reading now {#reading}

At the top are your profiles with a switch each. Every profile that is switched on reads its channels; several at once are fine (for example your own channel and a collaboration). **Chat connected** shows that TTV-TTS is in the chat; each channel shows *joining …* until Twitch has let it in.

## Speaking now and up next {#queue}

- **Speaking now** – the message being read, with the viewer's name, the voice, the language and, if it was detected, the emotion (for example *joy (normal)*). Translated messages show the translation below.
- **Up next** – messages waiting their turn. A message that waits too long or is too long is dropped according to the limits in the [profile](profiles#limits).
- **Recently** – what was read, skipped or not read, with the reason (for example *ignored user (bot)* or *blocked word*) and how long it took until the first sound.

### Buttons {#buttons}

| Button | What it does |
|---|---|
| **Skip** | stops the current message; the next one starts |
| **Pause** / **Resume** | new messages wait in the queue; nothing is lost |
| **Clear queue** | drops everything that's waiting |
| **Play again** | repeats one of the last 20 messages (within 3 minutes) – instantly, without generating it again |
| **Mute** (next to a name) | that viewer is no longer read in this profile |

The same actions are available as [global hotkeys](settings#hotkeys), as [chat commands for mods](profiles#mod-commands) and through the [remote control API](remote-api).

## Approving messages {#approval}

For sensitive streams, switch on **Approve messages before they are read** in the profile. Every message then waits under **Waiting for your OK** until you click **Read it** or **Drop it** (or press the hotkey). Messages nobody approves are dropped after the time set in the profile.

![Messages waiting for approval](live-approval.webp)

Announcements of subs, raids and other events don't need an OK.

## Test message {#test}

Under **Test message** you can send any text under any name. It goes exactly the way a chat message would – handy to try a voice, a filter or an emote before the stream.

## When nothing is read {#nothing}

- No profile is switched on → *No profile is on*.
- No voice host → *TTV-TTS can read the chat, but not speak.* See [Connect Voice Creator Pro](voice-host).
- A message appears under *Recently* as *not read* → the reason is shown next to it. See [Troubleshooting](troubleshooting).
