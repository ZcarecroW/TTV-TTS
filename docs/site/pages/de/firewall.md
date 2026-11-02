---
title: Firewall
description: TTV-TTS zum Proxy durchlassen – Windows-Firewall, Windows Firewall Control, Sicherheitspakete, macOS und Linux.
icon: ShieldCheck
---

TTV-TTS auf deinem Streaming-PC spricht mit dem Proxy über **Port 8110** (TCP für die Verbindung, UDP, damit es den Proxy findet). Die Firewall auf dem **PC des Proxys** muss das hereinlassen. Auf dem Streaming-PC muss sich nichts ändern, und auch nichts an deinem Router.

## Der schnelle Weg {#check}

Öffne den Proxy → **Firewall & Netzwerk**. Er prüft:

- ob die Regel **„TTV-TTS Proxy“** existiert, eingeschaltet ist und auf die richtige Programmdatei zeigt,
- ob andere Regeln den Proxy **blockieren** (Windows legt solche an, wenn du bei seiner Frage „Zugriff zulassen“ auf *Abbrechen* geklickt hast),
- ob dein Netzwerk auf **Öffentlich** steht (eine Regel für private Netze gilt dort nicht),
- ob **Windows Firewall Control** oder eine andere Firewall installiert ist.

![Die Firewall-Prüfung des Proxys](proxy-firewall.webp)

**In privaten Netzen erlauben** (oder **In allen Netzen erlauben**) behebt es – Windows fragt einmal nach Administratorrechten.

## Windows-Defender-Firewall von Hand {#windows}

1. Drück die Windows-Taste, tipp „Windows Defender Firewall mit erweiterter Sicherheit“ und öffne es.
2. **Eingehende Regeln → Neue Regel … → Programm**, Pfad: die Programmdatei des Proxys (steht auf seiner Firewall-Seite) → **Verbindung zulassen**.
3. Haken bei **Domäne** und **Privat** (und **Öffentlich** nur, wenn dein Heimnetz als öffentlich eingestuft ist) → Name „TTV-TTS Proxy“.
4. Hat Windows früher die Frage „Zugriff zulassen“ gezeigt und du hast **Abbrechen** geklickt, hat es Sperrregeln angelegt: Lösch eingehende Regeln namens „TTV-TTS Proxy“ mit rotem Symbol.

## Windows Firewall Control (Binisoft / Malwarebytes) {#wfc}

WFC arbeitet über der Windows-Firewall und entfernt eventuell Regeln, die es nicht kennt.

1. Zeigt WFC eine Meldung für „TTV-TTS Proxy“, wähl **Allow** (eingehend).
2. Mit **Secure Rules**: WFC → **Security** → *Define here a new authorized group* → `TTV-TTS` eintragen → **+** klicken. Dann im Proxy noch einmal auf **Erlauben …** klicken.
3. WFC → **Rules Panel** → nach „TTV-TTS“ suchen: Die Regel muss eingeschaltet sein und eingehende Verbindungen erlauben.
4. Das Filterprofil *Medium* blockiert unbekannte Programme; die Erlauben-Regel reicht, das Profil muss nicht geändert werden.

## Sicherheitspakete {#suites}

Norton, Bitdefender, ESET, Kaspersky, Avast, AVG, McAfee, G DATA, Avira und andere ersetzen die Windows-Firewall durch ihre eigene. Erlaube dort die Programmdatei des Proxys für eingehende Verbindungen in privaten Netzen:

| Paket | Wo |
|---|---|
| Norton | Einstellungen → Firewall → Programmsteuerung → Hinzufügen |
| Bitdefender | Schutz → Firewall → Einstellungen → Regeln → Regel hinzufügen (Zulassen, Heim/Büro) |
| ESET | Einstellungen → Netzwerkschutz → Firewall → Regeln bearbeiten → Hinzufügen (Eingehend, Zulassen) |
| Kaspersky | Einstellungen → Sicherheitseinstellungen → Firewall → Programme → den Proxy auf *Vertrauenswürdig* setzen |
| Avast / AVG | Schutz → Firewall → Anwendungseinstellungen → Neue Anwendungsregel |
| McAfee | Firewall → Programmberechtigungen → Vollzugriff erlauben |
| G DATA | Firewall → Regelsätze → dein Netzwerk → Neue Regel (eingehend, zulassen) |
| Andere (ZoneAlarm, Comodo, GlassWire, simplewall, Sophos) | eingehend TCP und UDP Port 8110 für den Proxy erlauben |

Die genauen Bezeichnungen hängen von Version und Sprache des Pakets ab.

## macOS {#macos}

Beim ersten Mal fragt macOS *„Soll das Programm TTV-TTS Proxy eingehende Netzwerkverbindungen akzeptieren?“* – klick auf **Erlauben**. Später: **Systemeinstellungen → Netzwerk → Firewall → Optionen → TTV-TTS Proxy → Eingehende Verbindungen erlauben**. Mit Little Snitch oder LuLu erlaubst du dort eingehende Verbindungen für TTV-TTS Proxy.

## Linux {#linux}

```sh
# ufw
sudo ufw allow from 192.168.0.0/16 to any port 8110 proto tcp
sudo ufw allow from 192.168.0.0/16 to any port 8110 proto udp
# firewalld
sudo firewall-cmd --permanent --zone=home --add-port=8110/tcp --add-port=8110/udp
sudo firewall-cmd --reload
```

## Die Verbindung testen {#test}

Auf dem Streaming-PC sollte TTV-TTS → **Stimmen-Host** den Proxy unter *Im Netzwerk gefunden* zeigen. Ohne TTV-TTS, in PowerShell:

```powershell
Test-NetConnection 192.168.1.25 -Port 8110
```

(mit der Adresse des Proxy-PCs). `TcpTestSucceeded : True` heißt, die Firewall lässt durch. Oder öffne dort im Browser `https://192.168.1.25:8110/ttv/v1/info` – eine Zertifikatswarnung ist normal, der Proxy nutzt sein eigenes Zertifikat.

## Router {#router}

Nichts zu tun: Beide PCs sind im selben Netz. **Leite Port 8110 (oder Port 8100 von Voice Creator Pro) nie ins Internet weiter.** Um Stimmen mit Freunden zu teilen, hat der Proxy einen eigenen, begrenzten Internet-Port – siehe [Freunde über das Internet](friends). Um den Proxy selbst von unterwegs zu nutzen, nimm ein VPN wie Tailscale oder WireGuard.
