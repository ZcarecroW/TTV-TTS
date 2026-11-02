---
title: OBS-Links
description: Die Stimmen als eigene OBS-Quelle – von diesem PC, vom Stimmen-PC oder im OBS eines Freundes über das Internet.
icon: Tv
---

Statt auf einem Soundgerät zu spielen, kann ein Profil seine Stimmen **als Browserquelle an OBS** schicken. Du bekommst die Stimmen als eigene Quelle mit Regler, Filtern und Mithören, ohne Desktop-Audio-Aufnahme oder virtuelle Kabel. Mit einem Stimmen-PC tut der Gaming-PC für die Stimmen gar nichts mehr.

Das wählst du pro Profil: **Profile → Wo die Stimmen spielen**.

## Drei Orte zum Abspielen {#where}

| Einstellung | Wer den Chat liest | Was OBS öffnet | Gut für |
|---|---|---|---|
| **Soundgerät dieses PCs** | TTV-TTS auf diesem PC | nichts (OBS nimmt das Desktop-Audio) | das einfache Setup |
| **OBS auf diesem PC** | TTV-TTS auf diesem PC | `http://127.0.0.1:8131/o/…` | die Stimmen als eigene OBS-Quelle |
| **OBS über den Stimmen-PC** | TTV-TTS Proxy auf dem zweiten PC | `http://192.168.…:8112/o/…` | Gaming-PCs (sie tun nichts für die Stimmen), Streams von Freunden |

Die Seite hinter einem Link zeigt nichts an und lädt nichts außer den Stimmen: etwa 1 KB Skript und eine Verbindung, die nur Ton überträgt, während jemand spricht. In OBS spielt sie sofort. Ein normaler Browser spielt sie erst nach einem Klick auf die Seite, weil Browser Ton bis dahin blockieren.

![Wo die Stimmen spielen: OBS auf diesem PC](profile-output.webp)

## Den Link in OBS einfügen {#add}

1. In TTV-TTS: **Profile** → das Profil → **Wo die Stimmen spielen** → **OBS auf diesem PC** (oder **OBS über den Stimmen-PC**).
2. Den Link kopieren.
3. In OBS: **Quellen → + → Browser**. Den Link als **URL** einfügen. Breite und Höhe sind egal.
4. **Audio über OBS steuern** anhaken. Die Stimmen erscheinen dann im **Audio-Mixer** als eigene Quelle: Regler, Filter (Kompressor, Limiter, EQ …) und *Audio-Monitoring*, wenn du sie auf dem Kopfhörer hören willst.
5. **Quelle herunterfahren, wenn nicht sichtbar** ausgeschaltet lassen. Sonst verstummen die Stimmen, wenn du in eine Szene ohne die Quelle wechselst. Am einfachsten: Die Quelle in eine Szene legen, die du in alle anderen einbettest, oder in jede Szene einfügen.

Die Karte in TTV-TTS zeigt **OBS verbunden**, sobald OBS den Link geöffnet hat. Dann schick auf der Seite [Live](live) eine Testnachricht.

> [!TIP]
> **OBS auf einem anderen PC** (einem eigenen Streaming-PC)? Schalte **Auch für andere PCs** ein und nimm den Link mit der Netzwerkadresse dieses PCs. Windows fragt eventuell einmal, ob TTV-TTS Verbindungen annehmen darf; mit Windows Firewall Control erlaube eingehende Verbindungen für TTV-TTS (siehe [Firewall](firewall)).

**Neuer Link** ersetzt den Link: Der alte funktioniert sofort nicht mehr. Mach das, falls ein Link je in falsche Hände geraten ist.

## Über den Stimmen-PC {#host}

Mit **OBS über den Stimmen-PC** übergibt TTV-TTS das Profil an TTV-TTS Proxy, zusammen mit den Stimmen seines Presets und den Stimmen, die deine Zuschauer schon haben.

![Wo die Stimmen spielen: OBS über den Stimmen-PC, mit dem Link vom Stimmen-PC](profile-output-host.webp)

Ab dann:

- **Der Stimmen-PC liest den Chat selbst**, erzeugt die Stimmen, wendet ihre Effekte an und schickt sie direkt an OBS. Der Gaming-PC ist nicht beteiligt; TTV-TTS darf sogar geschlossen sein.
- Er liest nur, **solange OBS den Link geöffnet hat**, und hört eine Minute nach dem Schließen auf. Der Link eines Freundes kostet nichts, solange er offline ist.
- Änderungen am Profil in TTV-TTS werden mitgeschickt. Die Seite [Live](live) zeigt, was der Stimmen-PC gerade sagt, mit Pause, Überspringen, Leeren und Testnachrichten.
- Das Proxy-Fenster listet diese Profile auf seiner Seite **OBS-Links**.

![Ein Profil, das der Stimmen-PC abspielt, auf der Seite Live](live-hosted.webp)

![Die Seite OBS-Links des Proxys](proxy-obs.webp)

Was auf dem Gaming-PC bleibt und für diese Profile nicht gilt: [Audio-Plugins](effects#plugins), die [Twitch-Anmeldung](settings#twitch) (Follows und Kanalpunkte-Belohnungen ohne Text), Übersetzen mit DeepL, die Systemstimme als Ersatz, Hotkeys und die [Fernsteuerung](remote-api). Subs, Raids und Cheers werden wie gewohnt angesagt: Sie kommen mit dem Chat.

## Für einen Freund: über das Internet {#online}

Leih deine Stimmen: Dein Freund fügt **einen Link** in sein OBS ein. Dein Stimmen-PC liest seinen Chat und schickt die Stimmen an sein OBS. Er installiert nichts und braucht keine Grafikkarte.

1. **Ein Profil für deinen Freund**: Leg in TTV-TTS ein neues Profil an, trag den Twitch-Kanal deines Freundes ein und wähl die Stimmen, die er bekommen soll.
2. **Wo die Stimmen spielen** → **OBS über den Stimmen-PC** → **Auch aus dem Internet** einschalten.
3. **Ein Name: DuckDNS.** Deine Internetadresse ändert sich ab und zu. [DuckDNS](https://www.duckdns.org) gibt dir kostenlos einen festen Namen wie `meinstream.duckdns.org`: mit GitHub oder Google anmelden, einen Namen wählen und den Token kopieren. Trag beides auf der Seite **Internet** des Proxys ein (wie [für den Zugang von Freunden](friends#duckdns)); der Proxy hält den Namen auf dich gerichtet.
4. **Den Port im Router öffnen**: Leite den **TCP-Port 8112** (den OBS-Port) an den Stimmen-PC weiter. Bei einer FRITZ!Box: Internet → Freigaben → Portfreigaben → den Stimmen-PC → Neue Freigabe → TCP, Port 8112 an Port 8112. Andere Router nennen es *Portweiterleitung* oder *virtueller Server*.
5. Den **Internet-Link** (`http://meinstream.duckdns.org:8112/o/…`) kopieren und deinem Freund per Privatnachricht schicken.

> [!WARNING]
> **Der Link ist das Passwort.** Er hat 16 zufällige Zeichen. Eine Adresse, die falsche Links probiert, wird für 15 Minuten gesperrt, und ein Link kann nur den Ton dieses einen Profils abspielen: Er kann nichts ändern und kommt weder an Voice Creator Pro noch in dein Netzwerk. OBS-Links nutzen einfaches HTTP, weil OBS das eigene Zertifikat des Proxys nicht annimmt. Wer den Netzwerkverkehr deines Freundes mitlesen kann, könnte also den Link sehen und mithören. Gib ihn nur Leuten, denen du vertraust, und ersetze ihn bei Bedarf mit **Neuer Link**. Links ohne **Auch aus dem Internet** lehnen jede Adresse außerhalb deines Netzwerks ab.

Dein Stimmen-PC muss an sein, während dein Freund streamt; Wake-on-LAN und Autostart helfen ([Stimmen-Host](voice-host)). Nachrichten für den Stream eines Freundes behandelt er wie deinen eigenen Chat: Sie kommen vor dem Berechnen von Stimmen, das dann pausiert.

Keine öffentliche IPv4-Adresse (DS-Lite)? Dann klappt die Portweiterleitung über IPv4 nicht. Frag deinen Anbieter nach einer öffentlichen IPv4-Adresse, oder nimm [Tailscale](friends#tailscale), wenn dein Freund es installieren kann.

Du kannst auch **OBS auf diesem PC** aus dem Internet erreichbar machen (Port 8131 an den Gaming-PC weiterleiten und deinen DuckDNS-Namen als **Adresse aus dem Internet** eintragen). Der Stimmen-PC ist aber die bessere Wahl: Dann hängt der Stream deines Freundes nicht davon ab, dass dein Gaming-PC läuft.

## Wenn etwas nicht klappt {#troubleshooting}

- **OBS nicht verbunden**: Prüf den Link in den Eigenschaften der Quelle in OBS und drück **Cache der aktuellen Seite aktualisieren**. Für einen anderen PC: **Auch für andere PCs** an, und die Firewall muss eingehende Verbindungen erlauben (Port 8131 für diesen PC, 8112 für den Stimmen-PC).
- **Verbunden, aber kein Ton in OBS**: **Audio über OBS steuern** angehakt? Steht der Regler der Quelle im Audio-Mixer oben, und ist sie nicht stumm?
- **Nachrichten melden „niemand hört zu (OBS ist nicht verbunden)“**: Das Profil spielt in OBS, aber keine Browserquelle hat seinen Link geöffnet. TTV-TTS lässt Voice Creator Pro dann nicht umsonst arbeiten.
- **Eine Testnachricht auf dem Stimmen-PC klappt nicht**: Er liest nur, solange OBS zuhört. Öffne zuerst den Link in OBS.
- **Der Link aus dem Internet öffnet sich nicht**: Sagt die Seite **Internet** des Proxys *Erreichbar*? Dann prüf die Weiterleitung von Port **8112** (nicht 8111, der ist für das TTV-TTS von Freunden).
