---
title: Einführung
description: Was TTV-TTS macht, wie es funktioniert und was du brauchst.
icon: Sparkle
---

**TTV-TTS** liest deinen Twitch-Chat mit natürlichen KI-Stimmen vor. Die Stimmen kommen aus **[Voice Creator Pro](https://voicecreator.pro)** (VCP), das auf deiner eigenen Grafikkarte läuft – auf deinem Streaming-PC oder auf einem zweiten Computer. Kein Cloud-Dienst, kein Abo, keine Kosten pro Nachricht und keine Twitch-Anmeldung nötig.

![Die Live-Seite: was gerade gelesen wird, was als Nächstes kommt, was gelesen wurde](live.webp "Die Live-Seite")

## Was es anders macht

- **Jeder Zuschauer hat seine eigene Stimme** und behält sie – in jeder Sprache. Ein Stammgast klingt heute genauso wie nächsten Monat.
- **Presets** sind Besetzungen von Figuren: 20 eingebaute (Fantasy Tavern, Starship Crew, Noir City …) mit je 240 unterschiedlichen Stimmen. Voice Creator Pro entwirft sie einmal; danach sind sie sofort bereit. Du kannst auch [eigene schreiben](presets#own-presets).
- **Jede Sprache**: Jede Nachricht wird in ihrer eigenen Sprache vorgelesen – oder [für dich übersetzt](profiles#translate), wenn du sie nicht sprichst.
- **Gefühle und Effekte**: Lach-Emotes lassen eine Stimme lachen, Heul-Emojis machen sie traurig; einige Stimmen klingen wie ein Radio, ein Roboter oder eine Höhle, und du kannst eigene [VST3- und CLAP-Plugins](effects#plugins) dazunehmen.
- **Du behältst die Kontrolle**: Filter, eine Freigabe-Warteschlange, Überspringen und Wiederholen, globale Tastenkürzel, Mod-Befehle im Chat und eine [Fernsteuerungs-API](remote-api) für Stream Deck und Streamer.bot.
- **Zwei PCs, kein Problem**: Der kleine [TTV-TTS Proxy](proxy) läuft neben Voice Creator Pro auf dem anderen PC, koppelt sich mit einem Code und hält die Verbindung verschlüsselt.

## So funktioniert es {#how-it-works}

1. TTV-TTS verbindet sich **anonym** mit dem Twitch-Chat (nur lesend, wie ein nicht angemeldeter Zuschauer).
2. Jede Nachricht wird aufgeräumt: Emotes, Links, Spam und Namen werden so behandelt, wie du es im [Profil](profiles) einstellst.
3. TTV-TTS wählt die Stimme des Zuschauers und die Sprache der Nachricht und bittet Voice Creator Pro zu sprechen – direkt auf diesem PC oder über den Proxy auf dem anderen.
4. Der Ton kommt zurück und spielt auf dem Ausgabegerät, das du gewählt hast. OBS nimmt ihn auf wie jeden anderen Ton.

Solange der Chat ruhig ist, berechnet Voice Creator Pro im Hintergrund die Stimmen deiner Presets. Kommt eine Nachricht, hat der Chat Vorrang.

## Was du brauchst {#requirements}

| | |
|---|---|
| **Voice Creator Pro** | 2.4 oder neuer, unter Windows oder macOS, mit eingeschalteter **HTTP-API** in seinen Einstellungen (Port 8100). |
| **Grafikkarte** | Eine, auf der Voice Creator Pro gut läuft. Gemessen auf einer RTX 3070: Eine Sekunde Sprache dauert etwa eine halbe Sekunde. |
| **Streaming-PC** | Windows 10/11, macOS 11+ oder Linux für TTV-TTS selbst. Es braucht wenig: Die Stimmen entstehen auf dem Stimmen-Host. |
| **Netzwerk** | Nur wenn VCP auf einem anderen PC läuft: beide im selben Heimnetz (oder siehe [Freunde über das Internet](friends)). |

> [!TIP]
> Du kommst von TTV-TTS 1.x, der OBS-Browserquelle? Deine alten Stimmen werden automatisch übernommen – siehe [Von 1.x umsteigen](from-v1).

## Nächste Schritte

- [TTV-TTS installieren](installation) (und den Proxy, wenn VCP auf einem anderen PC läuft).
- [Mit Voice Creator Pro verbinden](voice-host).
- [Den ersten Stream vorbereiten](first-stream).
