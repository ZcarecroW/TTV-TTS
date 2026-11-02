---
title: Coming from 1.x
description: From the OBS browser source to the desktop app – what changes and what stays.
icon: ClockRotate
---

TTV-TTS 1.x was a single HTML file used as an OBS browser source. 2.0 is a desktop app: it plays on an audio device instead of inside OBS, and it can do much more – presets, a proxy for a second PC, effects, translation, approvals, hotkeys.

## Your voices come along {#classic}

The 300 voices you made for 1.x (`TTS-de 01 …` to `TTS-ko 30 …`, 30 roles in 10 languages) are still in Voice Creator Pro. TTV-TTS finds them and offers on the **Presets** page: **Adopt as preset**. You get the preset **Classic 30** with the same roles – nothing has to be computed. Viewers can then be read with those voices like with any other preset.

Alternatively, the voice mode **role per viewer** in a [profile](profiles#voices) works exactly like 1.x: each viewer keeps a role, spoken by that role's voice of the message's language.

## Settings {#settings}

1.x was configured with URL parameters and a config block. 2.0 has the same options – and more – in [profiles](profiles): channels, roles, ignored users, emotes and emojis, links, name announcements, blocked words and replacements, limits, mod commands. Set them up once in the app.

## OBS {#obs}

Remove the TTV-TTS browser source from your OBS scenes (or keep it muted until you've switched). TTV-TTS 2.0 plays on an audio device that OBS records – see [Into OBS](first-stream#obs).

## Voice Creator Pro on a second PC {#lan}

With 1.x you may have run `vcp-lan-setup.ps1` on the Voice Creator Pro PC, which opened port 8101 with a Windows port proxy. 2.0 doesn't need it: the [TTV-TTS Proxy](proxy) replaces it, encrypted and with pairing. Remove the old setup on that PC – in an administrator PowerShell, in the folder with the script:

```powershell
powershell -ExecutionPolicy Bypass -File .\vcp-lan-setup.ps1 -Uninstall
```

It removes its scheduled task, its firewall rule, the port proxy and its helper files. (The script is still available at its old address if you no longer have it.)

## The old version {#old}

1.x keeps working at its old address – your OBS browser source doesn't break – but it isn't developed further. Its guide is still online: [TTV-TTS 1.x](https://zcarecrow.github.io/TTV-TTS/readme.html).
