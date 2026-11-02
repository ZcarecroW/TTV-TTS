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
| **DuckDNS, ifconfig.co** | your DuckDNS name and token; a reachability test | only with [friends over the internet](friends) or an [OBS link for the internet](obs#online) |
| **Let's Encrypt** | your DuckDNS name, to get its HTTPS certificate | only with [HTTPS for links from the internet](obs#https) |
| **Your router** (UPnP) | which ports to open to this PC | only with **Open the ports in the router by itself** on |

## What is stored {#stored}

- **Settings, profiles, viewer voices**: in the [data folder](installation#data) on your PC. Viewers are stored with their login, display name, chat color, voice, last message time and message count – the messages themselves are not kept.
- **Keys and tokens** (VCP's API key, the proxy pairing token, a DeepL key, the Twitch login, the DuckDNS token): in your system's credential store, never in plain files where a store exists.
- **Logs**: technical events (connections, errors, jobs), kept small.

## Between your PCs {#lan}

TTV-TTS and the proxy talk over HTTPS with the proxy's own certificate, which TTV-TTS checks on every connection (it learns it when pairing). Voice Creator Pro's own port is never opened to the network.

## Friends {#friends}

A friend you invite can use your computed voices through a separate port, with a limit per day. They can't see your chat, your settings or your other voices. You can remove them at any time.

## From the internet {#internet}

What you share on the internet – an [OBS link](obs#online) or [access for friends](friends) – is built to be put there:

- **Only what you share answers.** The router opens exactly those ports (UPnP) and closes them when you stop sharing. Settings, pairing and Voice Creator Pro's own API never answer from the internet.
- **Encrypted.** OBS links from the internet use HTTPS with a free certificate ([HTTPS](obs#https)); once it works, the same link over plain `http://` is refused. A friend's TTV-TTS checks the proxy's own certificate, which comes with the invite code.
- **Secrets nobody guesses.** An OBS link carries about 94 random bits, invite codes and friends' tokens about 190. An address on the internet that guesses wrong 20 times is locked out (OBS links: 15 minutes, keys and invites: a minute).
- **Floods don't take it down.** Connections are capped in total and per address; a request must arrive within 20 seconds, idle connections are closed, messages are size-limited. Friends have a rate and a daily limit.
- **Hostile chat doesn't either.** Anybody can type anything into a chat: every text path is tested with odd characters, broken emote data and endless lines.
- **A leaked link is replaced in one click:** **New link** in the profile makes the old one stop working at once.
