---
title: TTV-TTS Proxy
description: Die kleine App auf dem Voice-Creator-Pro-PC – was sie macht und jede Seite ihres Fensters.
icon: Network
---

Voice Creator Pro hört nur auf seinem eigenen PC (`127.0.0.1:8100`). Der **TTV-TTS Proxy** läuft daneben und lässt TTV-TTS von deinem Streaming-PC herein – verschlüsselt, nur nach dem Koppeln und nur für das, was TTV-TTS braucht. Außerdem passt er auf Voice Creator Pro auf, startet es neu, wenn es hängt oder langsam wird, und übernimmt das Berechnen von Presets – du kannst TTV-TTS dabei sogar schließen. Sollte sein eigenes Berechnen einmal stehen bleiben, während Voice Creator Pro nichts zu tun hat, startet er sich nach 20 Minuten selbst neu – tagelanges Berechnen läuft unbeaufsichtigt weiter.

Er sitzt im Infobereich (neben der Uhr); ein Klick auf das Symbol öffnet sein Fenster. Beim ersten Start verbindet ihn ein kurzer Assistent mit Voice Creator Pro, legt fest, wie sich Apps anmelden, und prüft die Firewall.

![Der erste Start des Proxys](proxy-setup.webp)

## Übersicht {#overview}

![Die Übersicht des Proxys](proxy-overview.webp)

Auf einen Blick: ob Voice Creator Pro läuft und seine HTTP-API antwortet, das geladene Modell und die Warteschlange, die gekoppelten Apps und offenen Verbindungen, die Adressen, die TTV-TTS nutzen kann, der Zertifikat-Code – und CPU, Arbeitsspeicher, Grafikkarte, VRAM, Temperatur und Leistung des PCs.

## Apps & Koppeln {#pairing}

![Apps & Koppeln](proxy-connect.webp)

- **Gerät koppeln** zeigt einen sechsstelligen Code, fünf Minuten gültig, und den Zertifikat-Code, den TTV-TTS zeigen muss. Siehe [Voice Creator Pro verbinden](voice-host#proxy).
- **Gekoppelte Geräte** – jedes TTV-TTS, das den Proxy nutzen darf, mit wann und woher es zuletzt da war. **Entfernen** nimmt den Zugang weg, bis es neu gekoppelt wird.
- **Zugangsschlüssel** – für Tools, die nicht koppeln können: entweder der Schlüssel von Voice Creator Pro oder ein eigener des Proxys. Gekoppelte Geräte brauchen ihn nicht.
- **Netzwerk-Optionen** – der Name des Proxys im Netzwerk, sein Port (8110), ob er automatisch gefunden werden kann, und ein optionaler unverschlüsselter Port für ältere Tools (standardmäßig aus).

## Voice Creator Pro {#vcp}

![Die Seite Voice Creator Pro im Proxy](proxy-vcp.webp)

- **Status** – Programm, HTTP-API, Prozess; **Starten**, **Schließen**, **Neu starten**.
- **API-Schlüssel von VCP** – hat Voice Creator Pro einen Schlüssel (dort *Einstellungen → HTTP API*), braucht ihn der Proxy. **Automatisch finden** liest ihn aus den Einstellungen von Voice Creator Pro; **VCP einen Schlüssel geben** setzt einen starken Zufallsschlüssel, falls es keinen hat. Apps sehen diesen Schlüssel nie, außer du wählst *Schlüssel von VCP* als Zugangsschlüssel.
- **Adresse der HTTP-API von VCP** – normalerweise `http://127.0.0.1:8100`.
- **Start & Wächter**:
  - **VCP zusammen mit dem Proxy starten** (standardmäßig aus), **VCP am Laufen halten** (startet es wieder, wenn es sich schließt).
  - **VCP neu starten, wenn es hängt** – nach der eingestellten Zeit ohne Antwort.
  - **VCP neu starten, wenn es deutlich langsamer wird** (standardmäßig aus) – siehe [Wenn Voice Creator Pro langsam wird](voice-host#slow). Das aktuelle und das übliche **Tempo** stehen hier.

## Internet {#internet}

Leih Freunden deine berechneten Stimmen über das Internet – siehe [Freunde über das Internet](friends).

## Firewall & Netzwerk {#firewall}

Prüft, ob andere PCs den Proxy erreichen, und behebt es mit einem Klick – siehe [Firewall](firewall).

## Protokoll {#log}

Was der Proxy gemacht hat: Verbindungen, Koppeln, Neustarts von Voice Creator Pro, Aufträge, Warnungen. **Alles kopieren** kopiert es für einen Fehlerbericht.

## Einstellungen {#settings}

![Die Einstellungen des Proxys](proxy-settings.webp)

- **Bei der Anmeldung starten** – der Proxy startet minimiert im Infobereich. Standardmäßig aus; schalte es für einen PC ein, der immer bereit sein soll.
- **Diesen PC wach halten, solange der Proxy arbeitet** (standardmäßig an) – solange er Stimmen berechnet, ein TTV-TTS verbunden ist, ein OBS-Link spielt oder ein Freund verbunden ist, geht der PC nicht in den Ruhezustand; der Bildschirm geht trotzdem wie eingestellt aus. Unter dem Schalter steht, warum er gerade wach bleibt. Gibt es nichts zu tun, entscheiden wieder die Energieeinstellungen. {#awake}
- **Updates** – Kanal *Stabil* oder *Vorschau*, **Updates automatisch installieren** (sobald der Proxy zehn Minuten nichts zu tun hatte).
- Aussehen & Sprache, wie in TTV-TTS.

## Ohne Fenster {#tray}

Schließt du das Fenster, läuft der Proxy im Infobereich weiter; **Beenden** im Menü des Symbols beendet ihn. TTV-TTS zeigt den Zustand des Proxys auf seiner Seite [Stimmen-Host](voice-host#status) und kann ihn von dort aktualisieren.
