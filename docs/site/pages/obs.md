---
title: OBS links
description: The voices as an OBS source of their own – from this PC, from the voice host PC, or in a friend's OBS over the internet.
icon: Tv
---

Instead of playing on a sound device, a profile can send its voices to **OBS as a browser source**. You get the voices as a source of their own, with a fader, filters and monitoring, without desktop-audio capture or virtual cables. With a voice host PC, the gaming PC doesn't do any work for them at all.

You choose it per profile: **Profiles → Where the voices play**.

## Three places to play {#where}

| Setting | Who reads the chat | What OBS opens | Good for |
|---|---|---|---|
| **This PC's sound device** | TTV-TTS on this PC | nothing (OBS picks up desktop audio) | the simple setup |
| **OBS on this PC** | TTV-TTS on this PC | `http://127.0.0.1:8131/o/…` | the voices as their own OBS source |
| **OBS through the voice host PC** | TTV-TTS Proxy on the second PC | `http://192.168.…:8112/o/…` | gaming PCs (they do nothing for the voices), friends' streams |

The page behind a link shows nothing and loads nothing but the voices: about 1 KB of script and one connection that only carries audio while somebody speaks. In OBS it plays right away. A normal browser plays it only after you click the page, because browsers block sound until then.

![Where the voices play: OBS on this PC](profile-output.webp)

## Add the link in OBS {#add}

1. In TTV-TTS: **Profiles** → the profile → **Where the voices play** → **OBS on this PC** (or **OBS through the voice host PC**).
2. Copy the link.
3. In OBS: **Sources → + → Browser**. Paste the link as the **URL**. Width and height don't matter.
4. Tick **Control audio via OBS**. The voices then appear in the **Audio Mixer** as their own source: fader, filters (compressor, limiter, EQ …) and *Audio Monitoring* if you want to hear them on your headphones.
5. Leave **Shutdown source when not visible** off. Otherwise the voices stop when you switch to a scene without the source. Simplest: put the source in a scene that you nest into all others, or add it to every scene.

The card in TTV-TTS shows **OBS connected** as soon as OBS has the link open. Then send a test message on the [Live](live) page.

> [!TIP]
> **OBS on another PC** (a separate streaming PC)? Switch on **Also for other PCs** and use the link with this PC's network address. Windows may ask once whether TTV-TTS may accept connections; with Windows Firewall Control, allow incoming connections for TTV-TTS (see [Firewall](firewall)).

**New link** replaces the link: the old one stops working at once. Do that if a link ever got into the wrong hands.

## Through the voice host PC {#host}

With **OBS through the voice host PC**, TTV-TTS hands the profile over to TTV-TTS Proxy, together with its preset's voices and the voices your viewers already have.

![Where the voices play: OBS through the voice host PC, with the link from the voice host PC](profile-output-host.webp)

From then on:

- **The voice host PC reads the chat itself**, generates the voices, applies their effects and sends them straight to OBS. The gaming PC isn't involved; TTV-TTS may even be closed.
- It reads only **while OBS has the link open**, and stops a minute after OBS closes it. A friend's link costs nothing while they're offline.
- Changes you make to the profile in TTV-TTS are sent along. The [Live](live) page shows what the voice host PC is saying, with pause, skip, clear and test messages.
- The proxy window lists these profiles on its **OBS links** page.

![A profile played by the voice host PC, on the Live page](live-hosted.webp)

![The OBS links page of the proxy](proxy-obs.webp)

[Audio plugins](effects#host-plugins) the voice host PC has installed too run there, with the settings from TTV-TTS. What stays on the gaming PC and doesn't apply to these profiles: plugins only installed there, the [Twitch login](settings#twitch) (follows and channel-point rewards without text), DeepL translation, the system voice as a fallback, hotkeys and the [remote control](remote-api). Subs, raids and cheers are announced as usual: they arrive with the chat.

## For a friend: over the internet {#online}

Lend your voices: your friend pastes **one link** into their OBS. Your voice host PC reads their chat and sends the voices to their OBS. They install nothing and need no graphics card.

1. **A profile for your friend**: in TTV-TTS create a new profile, enter your friend's Twitch channel and choose the voices they should get.
2. **Where the voices play** → **OBS through the voice host PC** → switch on **Also from the internet**.
3. **A name: DuckDNS.** Your internet address changes now and then. [DuckDNS](https://www.duckdns.org) gives you a free, fixed name like `mystream.duckdns.org`: sign in with GitHub or Google, pick a name and copy the token. Enter both in the proxy's **Internet** page (the same as [for friends' access](friends#duckdns)). The proxy keeps the name pointing at you (every 5 minutes, so the daily reconnect of many connections doesn't matter) and gets an **HTTPS certificate** for it by itself ([HTTPS](#https)).
4. **The port on your router**: the proxy opens **TCP port 8112** (the OBS port) itself with [UPnP](friends#router); the **Internet** page then says *Port … is open in the router*. If your router doesn't allow that, forward the port by hand: on a FRITZ!Box: Internet → Permit Access → Port Sharing → the voice host PC → New Sharing → TCP, port 8112 to port 8112. Other routers call it *port forwarding* or *virtual server*.
5. Wait until the link shows **HTTPS** (the first certificate takes about a minute), copy the **internet link** (`https://mystream.duckdns.org:8112/o/…`) and send it to your friend in a private message.

> [!WARNING]
> **The link is the password.** It has 16 random characters. An address that tries wrong links is locked out for 15 minutes, and a link can do nothing but play the audio of this one profile: it can't change anything and can't reach Voice Creator Pro or your network. With a DuckDNS name and its token the link uses **HTTPS**, so nobody on the way can see it or listen along; once that works, the plain `http://` version of the link no longer opens from the internet. Without DuckDNS (only your IP address) the link stays plain HTTP and says *not encrypted*. Only give it to people you trust, and replace it with **New link** if needed. Links without **Also from the internet** refuse every address outside your network.

Your voice host PC has to be on while your friend streams; Wake-on-LAN and autostart help ([Voice host](voice-host)). Messages for a friend's stream are treated like your own chat: they go before voice computations, which then pause.

No public IPv4 address (DS-Lite)? Then port forwarding can't work over IPv4. Ask your provider for a public IPv4 address, or use [Tailscale](friends#tailscale) if your friend can install it.

You can also make **OBS on this PC** reachable from the internet: enter your DuckDNS name as **Address from the internet** and its token. TTV-TTS then keeps the name pointing at this PC, opens port 8131 in the router (UPnP) and gets the HTTPS certificate itself, like the proxy. The voice host PC is the better choice, though: then your friend's stream doesn't depend on your gaming PC being on.

## HTTPS for links from the internet {#https}

HTTPS encrypts the link and the audio, so nobody between you and your friend can read the link (it is the password) or listen along. You don't have to know anything about certificates: with a **DuckDNS name and its token**, TTV-TTS and TTV-TTS Proxy do it themselves.

![HTTPS and UPnP on the proxy's Internet page](proxy-internet-https.webp)

What happens:

1. As soon as a link is shared on the internet, TTV-TTS asks [Let's Encrypt](https://letsencrypt.org) (a free, non-profit certificate authority that browsers and OBS trust) for a certificate for `yourname.duckdns.org`.
2. Let's Encrypt wants proof that the name is yours: a code in the name's DNS record. TTV-TTS puts it there through DuckDNS, waits until DuckDNS's name servers show it, and Let's Encrypt checks it. **No extra port** has to be open for this, and it doesn't matter that your internet address changes: the certificate is for the name.
3. The certificate and its key stay on the PC (the key never leaves it). The link changes to `https://…` – the first time takes about a minute.
4. Certificates last a few weeks (Let's Encrypt is making them shorter: 90 days today, 45 days from 2028). TTV-TTS renews them on its own when two thirds of the time have passed, or earlier when Let's Encrypt asks for it. Links and OBS keep running meanwhile.

The status line shows what's going on: *HTTPS certificate for … valid until …*, *Getting a free HTTPS certificate …* with the current step, or why it didn't work and when it tries again. **OBS links from the internet use HTTPS** switches it off (then the link is plain HTTP and says *not encrypted*). By using it you accept the [Let's Encrypt Subscriber Agreement](https://letsencrypt.org/repository/).

- **Links in your own network** stay plain `http://` (there's no public certificate for a LAN address, and nobody outside sees that traffic). The same port serves both.
- **Friends' TTV-TTS** (over [the internet port](friends)) was already encrypted before: its invite code carries the proxy's own certificate. That needs no Let's Encrypt certificate.
- **When it fails** (for example a wrong token: *DuckDNS said KO*), TTV-TTS tries again after 15 minutes, then less often – Let's Encrypt only allows a few failed checks per hour. Meanwhile the link stays plain HTTP, marked *not encrypted*.

## Troubleshooting {#troubleshooting}

- **OBS not connected**: check the link in OBS's source properties, then press **Refresh cache of current page**. For another PC: **Also for other PCs** on, and the firewall must allow incoming connections (port 8131 for this PC, 8112 for the voice host PC).
- **Connected, but no sound in OBS**: **Control audio via OBS** ticked? Is the source's fader up in the Audio Mixer, and not muted?
- **Messages say “nobody listens (OBS isn't connected)”**: the profile plays in OBS, but no browser source has its link open. TTV-TTS then doesn't make Voice Creator Pro work for nothing.
- **A test message on the voice host PC fails**: it only reads while OBS listens. Open the link in OBS first.
- **The link from the internet doesn't open**: does the proxy's **Internet** page say *Reachable* and *Ports … are open in the router*? Then check the forwarding of port **8112** (not 8111, which is for friends' TTV-TTS).
- **OBS shows “This TTV-TTS link needs https:// now”**: the link was copied before the certificate was there. Copy the new `https://` link from TTV-TTS into OBS.
