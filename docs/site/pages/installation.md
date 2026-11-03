---
title: Installation
description: Install TTV-TTS on your streaming PC – and the proxy on the Voice Creator Pro PC, if that's another one.
icon: Download
---

TTV-TTS comes as two apps:

| App | Where | When |
|---|---|---|
| **TTV-TTS** | your streaming PC | always |
| **TTV-TTS Proxy** | the PC with Voice Creator Pro | only if Voice Creator Pro runs on **another** PC |

Both are free downloads from the [releases page](https://github.com/ZcarecroW/TTV-TTS/releases/latest), for Windows, macOS and Linux, and both update themselves.

## Windows {#windows}

1. Download `TTV-TTS_<version>_x64-setup.exe` and run it. It installs for your user only – no administrator rights needed.
2. Windows may show **“Windows protected your PC”** (SmartScreen): TTV-TTS isn't signed with a paid certificate yet. Click **More info → Run anyway**.
3. TTV-TTS starts. On the first start it walks you through connecting to Voice Creator Pro.

The proxy installs the same way (`TTV-TTS-Proxy_<version>_x64-setup.exe`) on the other PC. It lives in the tray (next to the clock) and can start when you sign in.

## macOS {#macos}

1. Download the `.dmg` (one file for Apple Silicon and Intel), open it and drag TTV-TTS into **Applications**.
2. The first time, macOS refuses to open an app from an unidentified developer. Open **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to TTV-TTS.

## Linux {#linux}

- **AppImage** (updates itself): `chmod +x TTV-TTS_<version>_amd64.AppImage`, then start it.
- **.deb** (Debian, Ubuntu, Mint): `sudo apt install ./TTV-TTS_<version>_amd64.deb`
- **.rpm** (Fedora, openSUSE): `sudo dnf install ./TTV-TTS-<version>-1.x86_64.rpm`

TTV-TTS needs WebKitGTK 4.1 (installed with the .deb and .rpm). For the [system voice fallback](profiles#os-voice) install `espeak-ng`.

## Updates {#updates}

TTV-TTS checks for updates at start and every six hours and shows a notice; it installs when you click **Install and restart** – never in the middle of a stream. The proxy installs its updates by itself once it has been idle for ten minutes (you can switch that off in its settings), so an update never cuts into a stream either. Every update is signed; an update with a wrong signature is refused.

Under **Settings → Updates** you can choose the channel:

- **Stable** – the releases (recommended).
- **Preview** – test builds with the newest features, before they become a release.

The proxy can also be updated from TTV-TTS: **Voice host → Update proxy now**, so you don't have to walk to the other PC.

## Where your data lives {#data}

Settings, profiles and the voice library are stored in:

| | TTV-TTS | TTV-TTS Proxy |
|---|---|---|
| Windows | `%APPDATA%\io.github.zcarecrow.ttvtts` | `%APPDATA%\io.github.zcarecrow.ttvtts.proxy` |
| macOS | `~/Library/Application Support/io.github.zcarecrow.ttvtts` | `…/io.github.zcarecrow.ttvtts.proxy` |
| Linux | `~/.config/…` and `~/.local/share/io.github.zcarecrow.ttvtts` | `…/io.github.zcarecrow.ttvtts.proxy` |

Keys and tokens (VCP's API key, the pairing token, a DeepL key, the Twitch login) are not in these folders but in your system's credential store (Windows Credential Manager, macOS Keychain, Secret Service on Linux).

The voices themselves live in Voice Creator Pro – uninstalling TTV-TTS doesn't delete them.

## Uninstalling {#uninstall}

- Windows: **Settings → Apps → Installed apps → TTV-TTS → Uninstall**.
- macOS: drag TTV-TTS from Applications to the bin.
- Linux: delete the AppImage, or `sudo apt remove ttv-tts` / `sudo dnf remove ttv-tts`.

Delete the data folders above to remove your settings too.
