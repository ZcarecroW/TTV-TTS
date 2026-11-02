---
title: Remote control API
description: Skip, pause, replay or say something – from Stream Deck, Streamer.bot, Touch Portal or a script.
icon: Keyboard
---

**Settings → Remote control → Allow remote control** starts a small HTTP API on this PC (port **8130**, off by default). Each URL does one thing, so any tool that can open a URL can control TTV-TTS.

![Remote control in the settings](settings-remote.webp)

## Authentication {#token}

Every call needs the token shown in the settings – either in the URL (`?token=…`) or as a header (`Authorization: Bearer …`). Treat the URLs like a password. **New token** replaces it; old URLs stop working. Another PC that sends a wrong token 20 times is locked out for 15 minutes.

By default the API only answers on this PC (`127.0.0.1`). **Also from other PCs** opens it to your network (for a Stream Deck on another computer).

## Endpoints {#endpoints}

All actions answer `GET` and `POST` (Stream Deck's *Website* action can only open a URL).

| URL | What it does |
|---|---|
| `/api/status` | `{"connected":true,"paused":false,"queue":2,"pending":0,"speaking":"Name: text"}` |
| `/api/skip` | skip the current message |
| `/api/pause` / `/api/resume` | pause / resume |
| `/api/toggle-pause` | pause or resume; answers `paused` or `reading` |
| `/api/clear` | clear the queue |
| `/api/replay` | play the last message again |
| `/api/approve` / `/api/reject` | read / drop the next message waiting for approval |
| `/api/say?text=…&user=…` | read a text through the active profile (as a test message from *user*, default “StreamDeck”; at most 500 characters) – the text can also be sent as JSON: `{"text":"…","user":"…"}` |

Actions answer `{"ok":true}` or `{"ok":false,"message":"…"}`.

## Examples {#examples}

**Stream Deck** – action *Website*, URL `http://127.0.0.1:8130/api/skip?token=YOURTOKEN`, tick *GET request in background*.

**Streamer.bot** – sub-action *Fetch URL*: `http://127.0.0.1:8130/api/say?token=YOURTOKEN&user=%user%&text=%rawInput%`.

**PowerShell**

```powershell
Invoke-RestMethod "http://127.0.0.1:8130/api/toggle-pause?token=YOURTOKEN"
```

**curl**

```sh
curl -H "Authorization: Bearer YOURTOKEN" -d '{"text":"Welcome to the stream!"}' http://127.0.0.1:8130/api/say
```

Opening `http://127.0.0.1:8130/` in a browser shows a short help page.
