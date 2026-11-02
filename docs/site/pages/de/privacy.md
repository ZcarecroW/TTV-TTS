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
| **DuckDNS, ifconfig.co** | dein DuckDNS-Name und -Token; ein Erreichbarkeitstest | nur mit [Freunden über das Internet](friends) oder einem [OBS-Link fürs Internet](obs#online) |
| **Let's Encrypt** | dein DuckDNS-Name, um sein HTTPS-Zertifikat zu bekommen | nur mit [HTTPS für Links aus dem Internet](obs#https) |
| **Dein Router** (UPnP) | welche Ports zu diesem PC geöffnet werden | nur mit **Ports im Router selbst öffnen** an |

## Was gespeichert wird {#stored}

- **Einstellungen, Profile, Zuschauer-Stimmen**: im [Datenordner](installation#data) auf deinem PC. Zuschauer werden mit Login, Anzeigename, Chatfarbe, Stimme, Zeit der letzten Nachricht und Anzahl der Nachrichten gespeichert – die Nachrichten selbst nicht.
- **Schlüssel und Tokens** (API-Schlüssel von VCP, Kopplungs-Token des Proxys, ein DeepL-Schlüssel, die Twitch-Anmeldung, der DuckDNS-Token): im Anmeldedaten-Speicher deines Systems, nie in einfachen Dateien, wo es einen solchen Speicher gibt.
- **Protokolle**: technische Ereignisse (Verbindungen, Fehler, Aufträge), klein gehalten.

## Zwischen deinen PCs {#lan}

TTV-TTS und der Proxy sprechen über HTTPS mit dem eigenen Zertifikat des Proxys, das TTV-TTS bei jeder Verbindung prüft (es lernt es beim Koppeln). Der eigene Port von Voice Creator Pro wird nie für das Netzwerk geöffnet.

## Freunde {#friends}

Ein Freund, den du einlädst, kann deine berechneten Stimmen über einen eigenen Port nutzen, mit einem Tageslimit. Er sieht weder deinen Chat noch deine Einstellungen noch deine anderen Stimmen. Du kannst ihn jederzeit entfernen.

## Aus dem Internet {#internet}

Was du im Internet teilst – einen [OBS-Link](obs#online) oder [Zugang für Freunde](friends) –, ist dafür gebaut, dort zu stehen:

- **Nur, was du teilst, antwortet.** Der Router öffnet genau diese Ports (UPnP) und schließt sie wieder, wenn du nicht mehr teilst. Einstellungen, Kopplung und die eigene API von Voice Creator Pro antworten nie aus dem Internet.
- **Verschlüsselt.** OBS-Links aus dem Internet nutzen HTTPS mit einem kostenlosen Zertifikat ([HTTPS](obs#https)); sobald es klappt, wird derselbe Link über einfaches `http://` abgelehnt. Das TTV-TTS eines Freundes prüft das eigene Zertifikat des Proxys, das mit dem Einladungscode kommt.
- **Geheimnisse, die niemand errät.** Ein OBS-Link trägt rund 94 zufällige Bits, Einladungscodes und Tokens von Freunden rund 190. Eine Adresse aus dem Internet, die 20-mal falsch rät, wird gesperrt (OBS-Links: 15 Minuten, Schlüssel und Einladungen: eine Minute).
- **Fluten legen es nicht lahm.** Verbindungen sind insgesamt und pro Adresse begrenzt; eine Anfrage muss innerhalb von 20 Sekunden ankommen, untätige Verbindungen werden geschlossen, Nachrichten sind in der Größe begrenzt. Freunde haben ein Tempo- und ein Tageslimit.
- **Feindseliger Chat auch nicht.** In einen Chat kann jeder alles tippen: Jeder Textweg ist mit seltsamen Zeichen, kaputten Emote-Daten und endlosen Zeilen getestet.
- **Ein durchgesickerter Link ist mit einem Klick ersetzt:** **Neuer Link** im Profil lässt den alten sofort nicht mehr funktionieren.
