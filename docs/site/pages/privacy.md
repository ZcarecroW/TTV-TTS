---
title: Privacy
description: What TTV-TTS sends where – and what it keeps.
icon: Lock
---

TTV-TTS has no servers of its own and no telemetry. Everything happens on your PCs.

## What goes where {#flows}

| To | What | When |
|---|---|---|
| **Twitch** (chat, read-only) | nothing but joining your channels | always |
| **Your Voice Creator Pro** | the messages to be spoken | always |
| **BetterTTV, FrankerFaceZ, 7TV** | your channel's ID, to load its emotes | if those emote providers are on in the profile |
| **MyMemory** or **DeepL** | messages in languages you don't speak | only with [Translate for me](profiles#translate) and that service chosen |
| **Twitch** (login, EventSub) | your login token; Twitch sends follows and rewards | only with the optional [Twitch login](settings#twitch) |
| **GitHub** | an update check; ready-made voice downloads | update check every six hours; downloads when you click **Get** |
| **DuckDNS, ifconfig.co** | your DuckDNS name and token; a reachability test | only with [friends over the internet](friends) |

## What is stored {#stored}

- **Settings, profiles, viewer voices**: in the [data folder](installation#data) on your PC. Viewers are stored with their login, display name, chat color, voice, last message time and message count – the messages themselves are not kept.
- **Keys and tokens** (VCP's API key, the proxy pairing token, a DeepL key, the Twitch login, the DuckDNS token): in your system's credential store, never in plain files where a store exists.
- **Logs**: technical events (connections, errors, jobs), kept small.

## Between your PCs {#lan}

TTV-TTS and the proxy talk over HTTPS with the proxy's own certificate, which TTV-TTS checks on every connection (it learns it when pairing). Voice Creator Pro's own port is never opened to the network.

## Friends {#friends}

A friend you invite can use your computed voices through a separate port, with a limit per day. They can't see your chat, your settings or your other voices. You can remove them at any time.
