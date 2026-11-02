---
title: Connect Voice Creator Pro
description: The voice host – Voice Creator Pro on this PC, on another PC with the proxy, or a friend's.
icon: Server
---

TTV-TTS calls whatever speaks for it the **voice host**. On the first start, and later under **Voice host**, you choose one of three:

![Choosing the voice host on the first start](host-setup.webp "Three ways to a voice host")

## Voice Creator Pro on this PC {#direct}

If you stream and run Voice Creator Pro on the same computer:

1. Start Voice Creator Pro and make sure **HTTP API** is switched on in its settings (address `http://127.0.0.1:8100`).
2. In TTV-TTS choose **VCP runs on this PC**.
3. If you set an API key in Voice Creator Pro, enter it. Otherwise leave the field empty.

That's all – TTV-TTS talks to Voice Creator Pro directly.

## Voice Creator Pro on another PC {#proxy}

Many streamers keep the voices on a second PC, so the games on the streaming PC get the whole graphics card. Then:

1. Install the [TTV-TTS Proxy](installation) on the PC with Voice Creator Pro and start it. Switch on **HTTP API** in Voice Creator Pro's settings. The proxy finds Voice Creator Pro and, where possible, takes over its API key by itself.
2. In TTV-TTS choose **VCP runs on another PC**. The proxy shows up under **Found in your network** within a few seconds. If it doesn't, enter the other PC's address (for example `192.168.1.25`) – and see the [firewall guide](firewall).
3. Click **Pair**. TTV-TTS shows the proxy's certificate code – the proxy shows the same code. Compare them.
4. In the proxy click **Pair a device**. Type the six-digit code it shows into TTV-TTS.

![Pairing in the proxy: a six-digit code, valid for five minutes](proxy-pairing.webp "The proxy's pairing window")

From then on the connection is encrypted with the proxy's own certificate, and TTV-TTS only trusts that one. Pairing happens once; after a restart of either PC they find each other again.

> [!NOTE]
> The proxy uses port **8110** (TCP for the connection, UDP for finding it in the network). Voice Creator Pro's own port 8100 stays closed to the network – only the proxy on the same PC talks to it.

## A friend's voice host {#friend}

A friend can lend you their computed voices over the internet. They send you an **invite code** (it starts with `ttv1:`); choose **A friend's voice host** and paste it. See [Friends over the internet](friends) for how that works and what a guest can do.

## The Voice host page {#status}

![The Voice host page with connection, Voice Creator Pro and the PC's hardware](host.webp "Voice host")

- **Connection** – address, certificate, proxy version. **Update proxy now** installs a proxy update from here.
- **Voice Creator Pro** – running or not, version, the loaded model, the queue, and the **speed**: seconds of work per second of speech. Lower is faster; about 0.5 is typical for an RTX 3070.
- **Start VCP / Restart VCP** – the proxy starts or restarts Voice Creator Pro for you.
- **Wake PC** – sends a Wake-on-LAN signal to a sleeping or switched-off voice host PC (the network card must allow it; set in the BIOS and in Windows' network adapter settings).
- **PC with Voice Creator Pro** – CPU, memory, graphics card load, VRAM, temperature and power, live while the page is open.

### When Voice Creator Pro gets slow {#slow}

Voice Creator Pro can become slower the longer it runs; a restart brings it back. TTV-TTS measures the speed on every message. If it becomes clearly slower than usual, the proxy restarts Voice Creator Pro by itself at the next quiet moment in chat (at most every 30 minutes) and says so in its log. You can switch this off in the proxy under **Voice Creator Pro**.

### When the voice host is down {#down}

If Voice Creator Pro can't speak (stopped, restarting, PC off), TTV-TTS can read messages with your system's own voice instead of dropping them – see [System voice fallback](profiles#os-voice).
