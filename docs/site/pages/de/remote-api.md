---
title: Fernsteuerungs-API
description: Überspringen, pausieren, wiederholen oder etwas sagen lassen – von Stream Deck, Streamer.bot, Touch Portal oder einem Skript.
icon: Keyboard
---

**Einstellungen → Fernsteuerung → Fernsteuerung erlauben** startet auf diesem PC eine kleine HTTP-API (Port **8130**, standardmäßig aus). Jede URL macht genau eine Sache – jedes Tool, das eine URL öffnen kann, kann also TTV-TTS steuern.

![Fernsteuerung in den Einstellungen](settings-remote.webp)

## Anmeldung {#token}

Jeder Aufruf braucht den Token aus den Einstellungen – entweder in der URL (`?token=…`) oder als Header (`Authorization: Bearer …`). Behandle die URLs wie ein Passwort. **Neuer Token** ersetzt ihn; alte URLs gehen dann nicht mehr.

Standardmäßig antwortet die API nur auf diesem PC (`127.0.0.1`). **Auch von anderen PCs** öffnet sie für dein Netzwerk (für ein Stream Deck an einem anderen Computer).

## Endpunkte {#endpoints}

Alle Aktionen antworten auf `GET` und `POST` (die Aktion *Website* des Stream Decks kann nur eine URL öffnen).

| URL | Was sie macht |
|---|---|
| `/api/status` | `{"connected":true,"paused":false,"queue":2,"pending":0,"speaking":"Name: Text"}` |
| `/api/skip` | aktuelle Nachricht überspringen |
| `/api/pause` / `/api/resume` | pausieren / weiter |
| `/api/toggle-pause` | pausieren oder weiter; antwortet `paused` oder `reading` |
| `/api/clear` | Warteschlange leeren |
| `/api/replay` | die letzte Nachricht noch einmal abspielen |
| `/api/approve` / `/api/reject` | die nächste wartende Nachricht vorlesen / verwerfen |
| `/api/say?text=…&user=…` | einen Text über das aktive Profil vorlesen (als Testnachricht von *user*, standardmäßig „StreamDeck“; höchstens 500 Zeichen) – der Text geht auch als JSON: `{"text":"…","user":"…"}` |

Aktionen antworten `{"ok":true}` oder `{"ok":false,"message":"…"}`.

## Beispiele {#examples}

**Stream Deck** – Aktion *Website*, URL `http://127.0.0.1:8130/api/skip?token=DEINTOKEN`, Haken bei *GET request in background*.

**Streamer.bot** – Sub-Action *Fetch URL*: `http://127.0.0.1:8130/api/say?token=DEINTOKEN&user=%user%&text=%rawInput%`.

**PowerShell**

```powershell
Invoke-RestMethod "http://127.0.0.1:8130/api/toggle-pause?token=DEINTOKEN"
```

**curl**

```sh
curl -H "Authorization: Bearer DEINTOKEN" -d '{"text":"Willkommen im Stream!"}' http://127.0.0.1:8130/api/say
```

`http://127.0.0.1:8130/` im Browser zeigt eine kurze Hilfeseite.
