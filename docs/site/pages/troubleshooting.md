---
title: Troubleshooting
description: Nothing is read, the proxy can't be found, voices sound odd – what to check.
icon: Warning
---

## Nothing is read {#nothing-read}

1. **Live page**: is a profile switched on under *Reading now*, and does it say *Chat connected*? The channel name must be the Twitch login (the part after `twitch.tv/`).
2. **Recently** shows messages that were *not read* with the reason: ignored user, command prefix, blocked word, only certain roles, a channel-points reward filter, emote-only … Adjust it in the [profile](profiles#reading).
3. **Voice host**: *Not connected*? See below.
4. Still silent although messages show *spoken*? Check the output device under **Settings → Sound** and the volume (also `!ttsvol`).

## The proxy isn't found {#proxy-not-found}

- Is the proxy running on the other PC (tray icon)?
- Firewall: open the proxy → **Firewall & network** and click **Allow on private networks**. With Windows Firewall Control or a security suite, see [Firewall](firewall).
- Network type *Public*: a rule for private networks doesn't apply – allow on all networks, or set the network to private in Windows.
- Different networks (guest Wi-Fi, VLANs): automatic discovery only works within one network. Enter the address by hand.
- Test the port: [Test the connection](firewall#test).

## “VCP rejects the key” / Voice Creator Pro doesn't answer {#vcp}

- In Voice Creator Pro: **Settings → HTTP API** must be on.
- If Voice Creator Pro has an API key, the proxy (or TTV-TTS on the same PC) needs exactly that key: proxy → **Voice Creator Pro → Find automatically**, or paste it.
- The address must be `http://127.0.0.1:8100` unless you changed VCP's API port.

## The first message after a while is slow {#slow-first}

Voice Creator Pro keeps one model in memory. Designing a voice, speech recognition (the quality check) and translating with Voice Creator Pro push the speaking model out; the next message loads it again, which takes about 15–18 seconds. TTV-TTS therefore keeps those jobs away from chat: after a chat message, computing waits about 90 seconds before it designs again. During a busy chat, messages come in 1.3–1.8 s.

## Everything has become slow {#slow}

Voice Creator Pro can get slower the longer it runs. The **speed** on the [Voice host](voice-host#slow) page tells you: around 0.5 is normal on an RTX 3070; 2 or more means something is wrong. **Restart VCP** fixes it – and with **Restart Voice Creator Pro when it gets much slower** the proxy does that by itself at a quiet moment. The [benchmark](presets#benchmark) shows what your PC really manages.

## A voice sounds wrong {#odd-voice}

- Wrong gender or age, mumbling: [Design again](presets#redo) – or *let me pick from 3 takes*.
- The [quality check](presets#quality) lists voices that may sound odd; **Listen and decide**.
- A single viewer just sounds unfitting: give them another voice on the [Viewers](viewers) page.

## Words or names are read strangely {#pronunciation}

Use **Pronunciation of names**, **Pronunciation of emotes** and **Replacements** in the [profile](profiles#cleanup). Replacements can be set per language.

## The wrong language {#language}

Very short messages (“lol”, “gg”) can't be detected reliably. **Remember a viewer's language** keeps the last detected language for a few minutes; **Default language** covers the rest.

## Computing doesn't move {#jobs}

- The job says *chat goes first*: it waits for a gap in chat – that's intended.
- *Paused*: a paused job holds the queue; resume it in the job list.
- Without the proxy (Voice Creator Pro on this PC), TTV-TTS has to stay open for jobs to run.

## Twitch login {#twitch}

- *This build has no Twitch client ID*: enter your own, see [Your own Twitch application](settings#twitch-app).
- Channel-point rewards aren't announced: Twitch offers channel points to Affiliates and Partners only.
- *The Twitch login has expired*: log in again (Twitch ends logins after a password change or 30 days without use).

## Logs {#logs}

TTV-TTS writes a log file next to its settings ([data folders](installation#data)); the proxy shows its log on the **Log** page (**Copy all**). Attach them when you [report a problem](https://github.com/ZcarecroW/TTV-TTS/issues).
