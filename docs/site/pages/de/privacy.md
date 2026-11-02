---
title: Datenschutz
description: Was TTV-TTS wohin schickt – und was es speichert.
icon: Lock
---

TTV-TTS hat keine eigenen Server und keine Telemetrie. Alles passiert auf deinen PCs.

## Was wohin geht {#flows}

| An | Was | Wann |
|---|---|---|
| **Twitch** (Chat, nur lesend) | nur das Betreten deiner Kanäle | immer |
| **Dein Voice Creator Pro** | die zu sprechenden Nachrichten | immer |
| **BetterTTV, FrankerFaceZ, 7TV** | die ID deines Kanals, um seine Emotes zu laden | wenn diese Emote-Anbieter im Profil an sind |
| **MyMemory** oder **DeepL** | Nachrichten in Sprachen, die du nicht sprichst | nur mit [Für mich übersetzen](profiles#translate) und diesem Dienst |
| **Twitch** (Anmeldung, EventSub) | dein Anmelde-Token; Twitch schickt Follows und Belohnungen | nur mit der optionalen [Twitch-Anmeldung](settings#twitch) |
| **GitHub** | eine Update-Prüfung; fertige Stimmen-Downloads | Update-Prüfung alle sechs Stunden; Downloads, wenn du **Holen** klickst |
| **DuckDNS, ifconfig.co** | dein DuckDNS-Name und -Token; ein Erreichbarkeitstest | nur mit [Freunden über das Internet](friends) |

## Was gespeichert wird {#stored}

- **Einstellungen, Profile, Zuschauer-Stimmen**: im [Datenordner](installation#data) auf deinem PC. Zuschauer werden mit Login, Anzeigename, Chatfarbe, Stimme, Zeit der letzten Nachricht und Anzahl der Nachrichten gespeichert – die Nachrichten selbst nicht.
- **Schlüssel und Tokens** (API-Schlüssel von VCP, Kopplungs-Token des Proxys, ein DeepL-Schlüssel, die Twitch-Anmeldung, der DuckDNS-Token): im Anmeldedaten-Speicher deines Systems, nie in einfachen Dateien, wo es einen solchen Speicher gibt.
- **Protokolle**: technische Ereignisse (Verbindungen, Fehler, Aufträge), klein gehalten.

## Zwischen deinen PCs {#lan}

TTV-TTS und der Proxy sprechen über HTTPS mit dem eigenen Zertifikat des Proxys, das TTV-TTS bei jeder Verbindung prüft (es lernt es beim Koppeln). Der eigene Port von Voice Creator Pro wird nie für das Netzwerk geöffnet.

## Freunde {#friends}

Ein Freund, den du einlädst, kann deine berechneten Stimmen über einen eigenen Port nutzen, mit einem Tageslimit. Er sieht weder deinen Chat noch deine Einstellungen noch deine anderen Stimmen. Du kannst ihn jederzeit entfernen.
