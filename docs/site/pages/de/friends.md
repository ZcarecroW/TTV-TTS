---
title: Freunde über das Internet
description: Leih Freunden deine berechneten Stimmen, während sie streamen – sicher, mit Einladungen und Grenzen.
icon: Link
---

Deine Freunde haben keine starke Grafikkarte oder kein Voice Creator Pro? Leih ihnen deine Stimmen. Sie nutzen deine berechneten Preset-Stimmen über einen **eigenen Internet-Port** deines Proxys; dein Netzwerk, deine Einstellungen und Voice Creator Pro selbst bleiben für sie geschlossen.

Eingerichtet wird es auf der Seite **Internet** des Proxys oder in TTV-TTS (**Stimmen-Host → Freunde über das Internet**) – in beiden dieselbe Karte.

![Die Seite Internet des Proxys](proxy-internet.webp)

## Was ein Freund darf {#limits}

- Mit **deinen berechneten Stimmen** sprechen – sonst nichts: keine Einstellungen, keine anderen Stimmen, keine Uploads, kein Berechnen.
- Höchstens **600 Zeichen pro Nachricht** und ein **Tageslimit**, das du pro Freund wählst (10.000, 30.000 oder 100.000 Zeichen).
- **Dein eigener Chat geht immer vor.**
- Falsche Codes sperren die Adresse des Absenders für eine Minute.

Du siehst bei jedem Freund den heutigen Verbrauch, wann und woher er zuletzt da war, und kannst den Zugang jederzeit entfernen.

## Einrichten {#setup}

Zuerst ein Name (DuckDNS – [OBS-Links aus dem Internet](obs#online) nutzen ihn auch), dann **Freunde über das Internet zulassen** einschalten. Der Proxy öffnet Port **8111** auf seinem PC (nur für eingeladene Freunde). Dann:

### 1. Ein Name: DuckDNS {#duckdns}

Deine Internet-Adresse ändert sich ab und zu. [DuckDNS](https://www.duckdns.org) ist kostenlos: anmelden (mit GitHub oder Google), einen Namen wählen und den **Token** kopieren. Trag beides in der Karte ein. Der Proxy hält `deinname.duckdns.org` auf dich gerichtet – alle 5 Minuten, eine neue Adresse nach der täglichen Zwangstrennung ist also nach wenigen Minuten bekannt. Er fragt den Router nach dessen Internetadresse (UPnP), so kann nicht einmal ein VPN auf diesem PC den Namen auf das VPN lenken. Der Token bleibt auf dem PC des Proxys, in seinem Anmeldedaten-Speicher.

### 2. Den Port am Router öffnen {#router}

Meist ist nichts zu tun: Mit **Ports im Router selbst öffnen (UPnP)** (standardmäßig an) bittet der Proxy den Router, genau die Ports dessen weiterzuleiten, was du teilst – 8111 für Freunde, 8112 für [OBS-Links](obs#online) –, und zwar an seinen eigenen PC. Er erneuert das alle 25 Minuten und schließt die Ports wieder, wenn sie nicht mehr gebraucht werden oder der Proxy beendet wird. Die Karte zeigt dann *Port 8111 ist im Router offen*.

- **FRITZ!Box**: Sie erlaubt das nur Geräten, die du freigibst: Internet → Freigaben → Portfreigaben → der PC des Proxys → **Selbstständige Portfreigaben für dieses Gerät erlauben**. Sonst sagt die Karte, dass der Router diesem PC keine Ports öffnen lässt.
- **UPnP im Router aus**, oder du machst es lieber selbst: Schalte **UPnP** in der Karte aus und leite den Port von Hand weiter, wie unten.

Von Hand: Leite **TCP-Port 8111** an den PC des Proxys weiter (seine Adresse steht auf der Seite [Stimmen-Host](voice-host#status), zum Beispiel `192.168.178.25`).

- **FRITZ!Box**: Internet → Freigaben → Portfreigaben → Gerät für Freigaben hinzufügen → der PC des Proxys → Neue Freigabe → Portfreigabe, TCP, Port 8111 bis Port 8111, „IPv4 freigeben“.
- Andere Router nennen es *Portweiterleitung*, *Virtueller Server* oder *NAT*.
- **Windows-Firewall**: Die Regel des Proxys deckt Port 8111 schon ab. Windows Firewall Control oder eine andere Firewall: eingehend TCP 8111 für TTV-TTS Proxy erlauben.

> [!WARNING]
> Keine öffentliche IPv4-Adresse (DS-Lite, häufig bei Kabel und Glasfaser)? Dann kann die Weiterleitung über IPv4 nicht funktionieren – die Karte sagt es dir, wenn die Internetadresse des Routers keine öffentliche ist. Frag deinen Anbieter nach einer öffentlichen IPv4-Adresse – oder nimm Tailscale (unten).

### 3. Aus dem Internet prüfen {#test}

**Aus dem Internet prüfen** bittet ifconfig.co, sich mit deiner Adresse auf Port 8111 zu verbinden. *Erreichbar unter …* heißt, Freunde kommen hinein.

### 4. Einen Freund einladen {#invite}

Trag ein, für wen es ist, wie lange die Einladung gilt (1, 7 oder 30 Tage) und sein Tageslimit, dann **Code erstellen**. Schick den Code (er beginnt mit `ttv1:`) deinem Freund – per Direktnachricht, nicht in einem öffentlichen Chat. Er funktioniert **einmal**: Dein Freund fügt ihn in TTV-TTS unter **Stimmen-Host → Stimmen-Host eines Freundes** ein, und der Proxy macht daraus seinen persönlichen Zugang. Nicht genutzte Einladungen kannst du zurückziehen.

Der Code enthält das Zertifikat deines Proxys, deshalb spricht das TTV-TTS deines Freundes nur mit *deinem* Proxy – niemand kann sich dazwischenschalten.

## Ohne offenen Port: Tailscale {#tailscale}

Mit [Tailscale](https://tailscale.com) (privat kostenlos) ist nichts zum Internet offen: Nimm beide PCs in dein Tailnet auf, teile den PC des Proxys mit deinem Freund und trag seinen Tailscale-Namen unter **Andere Adresse (z. B. Tailscale)** ein, bevor du die Einladung erstellst.

## Für den Freund {#guest}

Füg den Einladungscode in TTV-TTS ein (**Stimmen-Host → Stimmen-Host eines Freundes**). TTV-TTS zeigt, wessen Stimmen-Host es ist und wie viel vom heutigen Limit du verbraucht hast. Anhören und Preset-Stimmen funktionieren wie gewohnt; Berechnen, Importieren und Qualitätsprüfungen passieren bei deinem Freund.
