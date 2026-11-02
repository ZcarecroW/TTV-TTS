---
title: Installation
description: TTV-TTS auf deinem Streaming-PC installieren – und den Proxy auf dem Voice-Creator-Pro-PC, falls das ein anderer ist.
icon: Download
---

TTV-TTS besteht aus zwei Apps:

| App | Wo | Wann |
|---|---|---|
| **TTV-TTS** | dein Streaming-PC | immer |
| **TTV-TTS Proxy** | der PC mit Voice Creator Pro | nur wenn Voice Creator Pro auf einem **anderen** PC läuft |

Beide gibt es kostenlos auf der [Versionsseite](https://github.com/ZcarecroW/TTV-TTS/releases/latest), für Windows, macOS und Linux, und beide aktualisieren sich selbst.

## Windows {#windows}

1. Lade `TTV-TTS_<Version>_x64-setup.exe` herunter und starte es. Es installiert nur für deinen Benutzer – keine Administratorrechte nötig.
2. Windows zeigt eventuell **„Der Computer wurde durch Windows geschützt“** (SmartScreen): TTV-TTS ist noch nicht mit einem kostenpflichtigen Zertifikat signiert. Klick auf **Weitere Informationen → Trotzdem ausführen**.
3. TTV-TTS startet. Beim ersten Start führt es dich durch die Verbindung mit Voice Creator Pro.

Der Proxy installiert sich genauso (`TTV-TTS-Proxy_<Version>_x64-setup.exe`) auf dem anderen PC. Er sitzt im Infobereich (neben der Uhr) und kann mit Windows starten.

## macOS {#macos}

1. Lade die `.dmg` herunter (eine Datei für Apple Silicon und Intel), öffne sie und zieh TTV-TTS in **Programme**.
2. Beim ersten Mal weigert sich macOS, eine App eines nicht verifizierten Entwicklers zu öffnen. Öffne **Systemeinstellungen → Datenschutz & Sicherheit**, scroll nach unten und klick neben TTV-TTS auf **Dennoch öffnen**.

## Linux {#linux}

- **AppImage** (aktualisiert sich selbst): `chmod +x TTV-TTS_<Version>_amd64.AppImage`, dann starten.
- **.deb** (Debian, Ubuntu, Mint): `sudo apt install ./TTV-TTS_<Version>_amd64.deb`
- **.rpm** (Fedora, openSUSE): `sudo dnf install ./TTV-TTS-<Version>-1.x86_64.rpm`

TTV-TTS braucht WebKitGTK 4.1 (wird mit .deb und .rpm installiert). Für die [Systemstimme als Ersatz](profiles#os-voice) installier `espeak-ng`.

## Updates {#updates}

TTV-TTS sucht beim Start und alle sechs Stunden nach Updates und zeigt einen Hinweis; installiert wird, wenn du auf **Installieren und neu starten** klickst – nie mitten im Stream. Der Proxy installiert seine Updates selbst, sobald er zehn Minuten nichts zu tun hatte (abschaltbar in seinen Einstellungen), damit auch er nie in einen Stream platzt. Jedes Update ist signiert; ein Update mit falscher Signatur wird abgelehnt.

Unter **Einstellungen → Updates** wählst du den Kanal:

- **Stabil** – die fertigen Versionen (empfohlen).
- **Vorschau** – Testversionen mit den neuesten Funktionen, bevor sie fertig sind.

Den Proxy kannst du auch aus TTV-TTS aktualisieren: **Stimmen-Host → Proxy jetzt aktualisieren** – du musst nicht zum anderen PC laufen.

## Wo deine Daten liegen {#data}

Einstellungen, Profile und die Stimmen-Bibliothek liegen in:

| | TTV-TTS | TTV-TTS Proxy |
|---|---|---|
| Windows | `%APPDATA%\io.github.zcarecrow.ttvtts` | `%APPDATA%\io.github.zcarecrow.ttvtts.proxy` |
| macOS | `~/Library/Application Support/io.github.zcarecrow.ttvtts` | `…/io.github.zcarecrow.ttvtts.proxy` |
| Linux | `~/.config/…` und `~/.local/share/io.github.zcarecrow.ttvtts` | `…/io.github.zcarecrow.ttvtts.proxy` |

Schlüssel und Tokens (der API-Schlüssel von VCP, das Kopplungs-Token, ein DeepL-Schlüssel, die Twitch-Anmeldung) liegen nicht in diesen Ordnern, sondern im Anmeldedaten-Speicher deines Systems (Windows-Anmeldeinformationsverwaltung, macOS-Schlüsselbund, Secret Service unter Linux).

Die Stimmen selbst liegen in Voice Creator Pro – wenn du TTV-TTS deinstallierst, bleiben sie erhalten.

## Deinstallieren {#uninstall}

- Windows: **Einstellungen → Apps → Installierte Apps → TTV-TTS → Deinstallieren**.
- macOS: TTV-TTS aus Programme in den Papierkorb ziehen.
- Linux: das AppImage löschen, oder `sudo apt remove ttv-tts` / `sudo dnf remove ttv-tts`.

Lösch die Datenordner oben, um auch deine Einstellungen zu entfernen.
