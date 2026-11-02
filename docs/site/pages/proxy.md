---
title: TTV-TTS Proxy
description: The small app on the Voice Creator Pro PC – what it does and every page of its window.
icon: Network
---

Voice Creator Pro only listens on its own PC (`127.0.0.1:8100`). The **TTV-TTS Proxy** runs next to it and lets TTV-TTS on your streaming PC in – encrypted, only after pairing, and only for what TTV-TTS needs. It also watches Voice Creator Pro, restarts it when it hangs or gets slow, and does the computing of presets, so you can even close TTV-TTS meanwhile.

It lives in the tray (next to the clock); click the icon to open its window. On its first start a short assistant connects it to Voice Creator Pro, sets how apps sign in and checks the firewall.

![The proxy's first start](proxy-setup.webp)

## Overview {#overview}

![The proxy's overview](proxy-overview.webp)

At a glance: whether Voice Creator Pro runs and its HTTP API answers, the loaded model and queue, the paired apps and open connections, the addresses TTV-TTS can use, the certificate code – and the PC's CPU, memory, graphics card, VRAM, temperature and power draw.

## Apps & pairing {#pairing}

![Apps & pairing](proxy-connect.webp)

- **Pair a device** shows a six-digit code, valid for five minutes, and the certificate code TTV-TTS must show. See [Connect Voice Creator Pro](voice-host#proxy).
- **Paired devices** – every TTV-TTS that may use the proxy, with when and from where it was last seen. **Remove** takes access away until it pairs again.
- **Access key** – for tools that can't pair: either Voice Creator Pro's own key or a key of the proxy's own. Paired devices don't need it.
- **Network options** – the proxy's name in the network, its port (8110), whether it can be found automatically, and an optional unencrypted port for older tools (off by default).

## Voice Creator Pro {#vcp}

![The Voice Creator Pro page of the proxy](proxy-vcp.webp)

- **Status** – program, HTTP API, process; **Start**, **Close**, **Restart**.
- **VCP's API key** – if Voice Creator Pro has a key (its *Settings → HTTP API*), the proxy needs it. **Find automatically** reads it from Voice Creator Pro's settings; **Give VCP a key** sets a strong random key if it has none. Apps never see this key unless you choose *Use VCP's key* as the access key.
- **Address of VCP's HTTP API** – normally `http://127.0.0.1:8100`.
- **Start & watchdog**:
  - **Start VCP together with the proxy**, **Keep VCP running** (starts it again if it closes).
  - **Restart VCP when it hangs** – after the time you choose without an answer.
  - **Restart VCP when it gets slow** – see [When Voice Creator Pro gets slow](voice-host#slow). The current and the usual **speed** are shown here.

## Internet {#internet}

Lend your computed voices to friends over the internet – see [Friends over the internet](friends).

## Firewall & network {#firewall}

Checks whether other PCs can reach the proxy and fixes it with one click – see [Firewall](firewall).

## Log {#log}

What the proxy did: connections, pairing, restarts of Voice Creator Pro, jobs, warnings. **Copy all** copies it for a bug report.

## Settings {#settings}

![The proxy's settings](proxy-settings.webp)

- **Start with Windows** (or *at login* on macOS).
- **Updates** – channel *Stable* or *Preview*, **Install updates automatically** (once the proxy has been idle for ten minutes).
- Look & language, as in TTV-TTS.

## Without a window {#tray}

Closing the window keeps the proxy running in the tray; **Quit** in the tray menu ends it. TTV-TTS shows the proxy's state on its [Voice host](voice-host#status) page and can update it from there.
