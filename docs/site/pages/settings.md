---
title: Settings
description: Sound, speed, hotkeys, remote control, the Twitch account, plugins and updates.
icon: Gear
---

![The Settings page](settings.webp)

## Sound {#sound}

- **Output device** – where TTV-TTS plays. *System default* follows your system; choose a virtual cable to send TTV-TTS only to OBS (see [Into OBS](first-stream#obs)).
- **Volume** – also settable from chat with `!ttsvol` ([mod commands](profiles#mod-commands)).
- **Pause between messages** – a short breath between two messages.

## Speech generation {#generation}

- **Whole sentences** – Voice Creator Pro generates each sentence completely, and the next one while the current one plays. On many PCs this is much faster overall.
- **Streaming** – audio starts while Voice Creator Pro is still generating: the first sound comes sooner, but the rest can be slower.
- **Automatic** (recommended) – TTV-TTS measures both on your voice host once chat has been quiet for two minutes, and uses the faster one. **Measure now** measures right away.

Measured on an RTX 3070 with Qwen3: whole sentences – first sound after about 2 s, generated at 0.56× real time; streaming – first sound after about 1 s, but generated at 4.3× real time, so longer messages stall. *Whole sentences* wins there.

## Translation service {#translation}

For [Translate for me](profiles#translate):

- **Email for MyMemory** (optional) – MyMemory is free; with an email address the daily limit is ten times higher.
- **DeepL API key** (optional) – a free DeepL API key works. It's stored in your system's credential store.

## Hotkeys {#hotkeys}

Global hotkeys work everywhere, also while a game or OBS has the focus: **Skip message**, **Pause / resume**, **Clear the queue**, **Play the last again**, **Approve the next** and **Drop the next** ([approval](live#approval)).

![Hotkeys](settings-hotkeys.webp)

Click a field and press the combination (with Ctrl, Alt or Shift – or an F key – so it doesn't fire while you type); Esc cancels. A combination another program already uses can't be taken.

## Remote control {#remote}

**Allow remote control** starts a small API on this PC for Stream Deck, Streamer.bot, Touch Portal and others. Each URL does one thing – see [Remote control API](remote-api).

![Remote control](settings-remote.webp)

## Twitch account (optional) {#twitch}

Chat is read without a login. Log in with Twitch only to also announce **follows** and **channel-point rewards without text** ([announcements](profiles#announcements)).

![Twitch account](settings-twitch.webp)

1. Click **Log in with Twitch**. Your browser opens twitch.tv/activate with a code, which TTV-TTS also shows.
2. Confirm it on Twitch. TTV-TTS notices by itself and shows *Logged in as …* with the events it receives.

Your password never goes through TTV-TTS. It may only see who follows you and which rewards are redeemed (`moderator:read:followers`, `channel:read:redemptions`). The login is kept in your system's credential store, checked every hour as Twitch requires, and **Log out** revokes it at Twitch.

### Your own Twitch application {#twitch-app}

To log in, an app needs a *client ID*: the name tag of an application registered at Twitch. TTV-TTS has one built in (“TTV-TTS by ZcarecroW”), so normally you just click **Log in with Twitch**. You can use your own instead – it's free and takes two minutes.

**Why you might want your own:**

- **You're independent.** Your login goes through an application only you control. If the built-in one were ever changed, blocked or replaced, your login keeps working.
- **It's yours to see and revoke.** At login Twitch shows *your* application's name, and under [Twitch → Settings → Connections](https://www.twitch.tv/settings/connections) it's listed as yours.
- **Builds you compile yourself** (see [Building from source](building)) have no client ID built in – there you need your own.
- **Nothing gets slower or faster**: Twitch counts its limits per user, so sharing the built-in application costs you nothing. If you're happy with the built-in one, there's no need to change.

**How:**

1. Twitch only lets accounts with **two-factor authentication** register applications – switch it on under [Settings → Security](https://www.twitch.tv/settings/security) if you haven't.
2. Open [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) (log in with your Twitch account) and choose **Register Your Application**.
3. **Name**: something unique, for example “TTV-TTS (your name)” – Twitch rejects names that are taken. **OAuth Redirect URLs**: `http://localhost` (TTV-TTS logs in with a code you confirm in the browser, so this address is never used). **Category**: *Application Integration*. **Client Type**: **Public**. Then **Create**.
4. Click **Manage** next to the new application and copy the **Client ID**. You don't need a client secret – a public application has none.
5. In TTV-TTS open **Settings → Twitch account**, expand **Use your own Twitch application**, paste the client ID and save. Then **Log in with Twitch**.

To go back to the built-in application, choose **Use the built-in one**. Your client ID isn't secret in the sense of a password (Twitch sends it along with every request), but there's no reason to post it publicly.

## Audio plugins {#plugins}

VST3 and CLAP effects on every voice – see [Effects & plugins](effects#plugins).

## Look & language {#look}

TTV-TTS speaks English and German and follows your system unless you choose. **Theme**: *System*, *Dark* (dark blue) or *Light*.

![The light theme](live-light.webp)

## Updates {#updates}

**Update channel** *Stable* or *Preview*, **Check now**, and **Install and restart** when an update is there – see [Updates](installation#updates).
