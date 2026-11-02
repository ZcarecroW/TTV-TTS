---
title: Voice Creator Pro verbinden
description: Der Stimmen-Host – Voice Creator Pro auf diesem PC, auf einem anderen PC mit dem Proxy, oder der eines Freundes.
icon: Server
---

Was für TTV-TTS spricht, heißt **Stimmen-Host**. Beim ersten Start und später unter **Stimmen-Host** wählst du eine von drei Möglichkeiten:

![Den Stimmen-Host beim ersten Start wählen](host-setup.webp "Drei Wege zum Stimmen-Host")

## Voice Creator Pro auf diesem PC {#direct}

Wenn du auf demselben Computer streamst, auf dem Voice Creator Pro läuft:

1. Starte Voice Creator Pro und sorg dafür, dass **HTTP API** in seinen Einstellungen eingeschaltet ist (Adresse `http://127.0.0.1:8100`).
2. Wähl in TTV-TTS **VCP läuft auf diesem PC**.
3. Hast du in Voice Creator Pro einen API-Schlüssel gesetzt, trag ihn ein. Sonst lass das Feld leer.

Das ist alles – TTV-TTS spricht direkt mit Voice Creator Pro.

## Voice Creator Pro auf einem anderen PC {#proxy}

Viele Streamer lassen die Stimmen auf einem zweiten PC laufen, damit die Spiele auf dem Streaming-PC die ganze Grafikkarte bekommen. Dann:

1. Installier den [TTV-TTS Proxy](installation) auf dem PC mit Voice Creator Pro und starte ihn. Schalte in den Einstellungen von Voice Creator Pro **HTTP API** ein. Der Proxy findet Voice Creator Pro und übernimmt, wo möglich, dessen API-Schlüssel selbst.
2. Wähl in TTV-TTS **VCP läuft auf einem anderen PC**. Der Proxy erscheint nach wenigen Sekunden unter **Im Netzwerk gefunden**. Falls nicht, gib die Adresse des anderen PCs ein (zum Beispiel `192.168.1.25`) – und lies die [Firewall-Anleitung](firewall).
3. Klick auf **Koppeln**. TTV-TTS zeigt den Zertifikat-Code des Proxys – der Proxy zeigt denselben Code. Vergleiche sie.
4. Klick im Proxy auf **Gerät koppeln**. Tipp den sechsstelligen Code, den er zeigt, in TTV-TTS ein.

![Koppeln im Proxy: ein sechsstelliger Code, fünf Minuten gültig](proxy-pairing.webp "Das Kopplungsfenster des Proxys")

Ab dann ist die Verbindung mit dem eigenen Zertifikat des Proxys verschlüsselt, und TTV-TTS vertraut nur diesem. Gekoppelt wird einmal; nach einem Neustart eines der PCs finden sie sich wieder.

> [!NOTE]
> Der Proxy nutzt Port **8110** (TCP für die Verbindung, UDP zum Finden im Netzwerk). Der eigene Port 8100 von Voice Creator Pro bleibt für das Netzwerk geschlossen – nur der Proxy auf demselben PC spricht mit ihm.

## Der Stimmen-Host eines Freundes {#friend}

Ein Freund kann dir seine berechneten Stimmen über das Internet leihen. Er schickt dir einen **Einladungscode** (er beginnt mit `ttv1:`); wähl **Stimmen-Host eines Freundes** und füg ihn ein. Wie das funktioniert und was ein Gast darf, steht unter [Freunde über das Internet](friends).

## Die Seite Stimmen-Host {#status}

![Die Seite Stimmen-Host mit Verbindung, Voice Creator Pro und der Hardware des PCs](host.webp "Stimmen-Host")

- **Verbindung** – Adresse, Zertifikat, Proxy-Version. **Proxy jetzt aktualisieren** installiert ein Proxy-Update von hier aus.
- **Voice Creator Pro** – läuft oder nicht, Version, das geladene Modell, die Warteschlange und das **Tempo**: Sekunden Arbeit pro Sekunde Sprache. Weniger ist schneller; etwa 0,5 ist für eine RTX 3070 typisch.
- **VCP starten / VCP neu starten** – der Proxy startet Voice Creator Pro für dich oder startet es neu.
- **PC aufwecken** – schickt ein Wake-on-LAN-Signal an einen schlafenden oder ausgeschalteten Stimmen-Host-PC (die Netzwerkkarte muss es erlauben; einzustellen im BIOS und in den Netzwerkadapter-Einstellungen von Windows).
- **PC mit Voice Creator Pro** – CPU, Arbeitsspeicher, Grafikkarten-Last, VRAM, Temperatur und Leistung, live, solange die Seite offen ist.

### Wenn Voice Creator Pro langsam wird {#slow}

Voice Creator Pro kann langsamer werden, je länger es läuft; ein Neustart bringt das Tempo zurück. TTV-TTS misst das Tempo bei jeder Nachricht. Wird es deutlich langsamer als sonst, startet der Proxy Voice Creator Pro im nächsten ruhigen Moment im Chat von selbst neu (höchstens alle 30 Minuten) und vermerkt das in seinem Protokoll. Abschalten kannst du das im Proxy unter **Voice Creator Pro**.

### Wenn der Stimmen-Host ausfällt {#down}

Kann Voice Creator Pro nicht sprechen (beendet, startet neu, PC aus), kann TTV-TTS Nachrichten mit der Stimme deines Systems vorlesen, statt sie zu verwerfen – siehe [Systemstimme als Ersatz](profiles#os-voice).
