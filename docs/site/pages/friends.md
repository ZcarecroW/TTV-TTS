---
title: Friends over the internet
description: Lend your computed voices to friends while they stream – safely, with invites and limits.
icon: Link
---

Your friends don't have a strong graphics card, or no Voice Creator Pro? Lend them your voices. They use your computed preset voices through a **separate internet port** of your proxy; your network, your settings and Voice Creator Pro itself stay closed to them.

You set it up in the proxy's **Internet** page, or from TTV-TTS (**Voice host → Friends over the internet**) – the same card in both.

![The Internet page of the proxy](proxy-internet.webp)

## What a friend can do {#limits}

- Speak with **your computed voices** – nothing else: no settings, no other voices, no uploads, no computing.
- At most **600 characters per message** and a **daily limit** you choose per friend (10,000, 30,000 or 100,000 characters).
- **Your own chat always goes first.**
- Wrong codes block the sender's address for a minute.

You see each friend's use today, when they were last seen and from where, and can remove access at any time.

## Setting it up {#setup}

First a name (DuckDNS – [OBS links from the internet](obs#online) use it too), then switch on **Allow friends over the internet**. The proxy opens port **8111** on its PC (for invited friends only). Then:

### 1. A name: DuckDNS {#duckdns}

Your internet address changes now and then. [DuckDNS](https://www.duckdns.org) is free: sign in (with GitHub or Google), pick a name and copy the **token**. Enter both in the card. The proxy keeps `yourname.duckdns.org` pointing at you – every 5 minutes, so a new address after the daily reconnect is out within minutes. It asks the router for its internet address (UPnP), so even a VPN on that PC can't point the name at the VPN. The token stays on the proxy's PC, in its credential store.

### 2. Open the port on your router {#router}

Usually there's nothing to do: with **Open the ports in the router by itself (UPnP)** (on by default) the proxy asks the router to forward exactly the ports of what you share – 8111 for friends, 8112 for [OBS links](obs#online) – to its own PC, renews that every 25 minutes and closes them again when they're no longer needed or the proxy quits. The card then says *Port 8111 is open in the router*.

- **FRITZ!Box**: it only allows that for devices you permit: Internet → Permit Access → Port Sharing → the proxy's PC → **Permit independent port sharing for this device**. Otherwise the card says that the router doesn't let this PC open ports.
- **UPnP off in the router**, or you prefer to do it yourself: switch **UPnP** off in the card and forward the port by hand, as below.

By hand: forward **TCP port 8111** to the proxy's PC (its address is on the [Voice host](voice-host#status) page, for example `192.168.178.25`).

- **FRITZ!Box**: Internet → Permit Access → Port Sharing → Add Device for Sharing → the proxy's PC → New Sharing → Port Sharing, TCP, port 8111 to port 8111, “Permit IPv4”.
- Other routers call it *Port forwarding*, *Virtual server* or *NAT*.
- **Windows Firewall**: the proxy's rule covers port 8111 already. Windows Firewall Control or another firewall: allow incoming TCP 8111 for TTV-TTS Proxy.

> [!WARNING]
> No public IPv4 address (DS-Lite, common with cable and fiber)? Then forwarding can't work over IPv4 – the card tells you when the router's internet address isn't a public one. Ask your provider for a public IPv4 address – or use Tailscale (below).

### 3. Check from the internet {#test}

**Check from the internet** asks ifconfig.co to connect to your address on port 8111. *Reachable at …* means friends can get in.

### 4. Invite a friend {#invite}

Enter who it's for, how long the invite is valid (1, 7 or 30 days) and their daily limit, then **Create code**. Send the code (it starts with `ttv1:`) to your friend – by DM, not in a public chat. It works **once**: your friend pastes it in TTV-TTS under **Voice host → A friend's voice host**, and the proxy turns it into their personal access. Unused invites can be withdrawn.

The code contains your proxy's certificate, so your friend's TTV-TTS only talks to *your* proxy – an impostor can't step in between.

## Without opening a port: Tailscale {#tailscale}

With [Tailscale](https://tailscale.com) (free for private use) nothing is open to the internet: put both PCs in your tailnet, share the proxy's PC with your friend, and enter its Tailscale name under **Another address** before creating the invite.

## For the friend {#guest}

Paste the invite code in TTV-TTS (**Voice host → A friend's voice host**). TTV-TTS shows whose voice host it is and how much of today's limit you've used. Listening and preset voices work as usual; computing, importing and quality checks happen on your friend's side.
