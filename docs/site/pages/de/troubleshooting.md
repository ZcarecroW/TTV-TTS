---
title: Fehlersuche
description: Nichts wird gelesen, der Proxy wird nicht gefunden, Stimmen klingen seltsam – was du prüfen kannst.
icon: Warning
---

## Nichts wird gelesen {#nothing-read}

1. **Live-Seite**: Ist unter *Liest gerade* ein Profil eingeschaltet, und steht da *Chat verbunden*? Der Kanalname muss der Twitch-Login sein (der Teil nach `twitch.tv/`).
2. **Zuletzt** zeigt Nachrichten, die *nicht gelesen* wurden, mit Grund: ignorierter Nutzer, Befehls-Präfix, gesperrtes Wort, nur bestimmte Rollen, Kanalpunkte-Filter, nur Emotes … Pass es im [Profil](profiles#reading) an.
3. **Stimmen-Host**: *Nicht verbunden*? Siehe unten.
4. Immer noch still, obwohl Nachrichten *gesprochen* zeigen? Prüf das Ausgabegerät unter **Einstellungen → Ton** und die Lautstärke (auch `!ttsvol`).

## Der Proxy wird nicht gefunden {#proxy-not-found}

- Läuft der Proxy auf dem anderen PC (Symbol im Infobereich)?
- Firewall: Öffne den Proxy → **Firewall & Netzwerk** und klick auf **In privaten Netzen erlauben**. Mit Windows Firewall Control oder einem Sicherheitspaket siehe [Firewall](firewall).
- Netzwerktyp *Öffentlich*: Eine Regel für private Netze gilt dort nicht – erlaube es in allen Netzen oder stell das Netzwerk in Windows auf privat.
- Verschiedene Netze (Gast-WLAN, VLANs): Die automatische Suche funktioniert nur innerhalb eines Netzes. Gib die Adresse von Hand ein.
- Den Port testen: [Die Verbindung testen](firewall#test).

## „VCP lehnt den Schlüssel ab“ / Voice Creator Pro antwortet nicht {#vcp}

- In Voice Creator Pro: **Einstellungen → HTTP API** muss an sein.
- Hat Voice Creator Pro einen API-Schlüssel, braucht der Proxy (oder TTV-TTS auf demselben PC) genau diesen: Proxy → **Voice Creator Pro → Automatisch finden**, oder einfügen.
- Die Adresse muss `http://127.0.0.1:8100` sein, außer du hast den API-Port von VCP geändert.

## Die erste Nachricht nach einer Weile ist langsam {#slow-first}

Voice Creator Pro hält ein Modell im Speicher. Stimmen entwerfen, Spracherkennung (die Qualitätsprüfung) und Übersetzen mit Voice Creator Pro verdrängen das Sprech-Modell; die nächste Nachricht lädt es wieder, das dauert etwa 15–18 Sekunden. Deshalb hält TTV-TTS solche Arbeit vom Chat fern: Nach einer Chatnachricht wartet das Berechnen etwa 90 Sekunden, bevor es wieder entwirft. In einem lebhaften Chat kommen Nachrichten in 1,3–1,8 s.

## Alles ist langsam geworden {#slow}

Voice Creator Pro kann langsamer werden, je länger es läuft. Das **Tempo** auf der Seite [Stimmen-Host](voice-host#slow) zeigt es: um 0,5 ist auf einer RTX 3070 normal; 2 oder mehr heißt, etwas stimmt nicht. **VCP neu starten** behebt es – und mit **Voice Creator Pro neu starten, wenn es deutlich langsamer wird** macht der Proxy das selbst in einem ruhigen Moment.

## Eine Stimme klingt falsch {#odd-voice}

- Falsches Geschlecht oder Alter, Nuscheln: [Neu entwerfen](presets#redo) – oder *ich wähle aus 3 Versuchen*.
- Die [Qualitätsprüfung](presets#quality) listet Stimmen, die seltsam klingen könnten; **Anhören und entscheiden**.
- Ein einzelner Zuschauer klingt einfach unpassend: Gib ihm auf der Seite [Zuschauer](viewers) eine andere Stimme.

## Wörter oder Namen werden seltsam gelesen {#pronunciation}

Nutz **Aussprache von Namen**, **Aussprache von Emotes** und **Ersetzungen** im [Profil](profiles#cleanup). Ersetzungen lassen sich pro Sprache festlegen.

## Die falsche Sprache {#language}

Sehr kurze Nachrichten („lol“, „gg“) lassen sich nicht sicher erkennen. **Sprache eines Zuschauers merken** behält die zuletzt erkannte Sprache ein paar Minuten; die **Standardsprache** deckt den Rest ab.

## Das Berechnen geht nicht weiter {#jobs}

- Der Auftrag sagt *Chat geht vor*: Er wartet auf eine Lücke im Chat – das ist gewollt.
- *Pausiert*: Ein pausierter Auftrag hält die Warteschlange auf; setz ihn in der Auftragsliste fort.
- Ohne Proxy (Voice Creator Pro auf diesem PC) muss TTV-TTS offen bleiben, damit Aufträge laufen.

## Twitch-Anmeldung {#twitch}

- *In dieser Version ist keine Twitch-Anwendung eingebaut*: Trag deine eigene ein, siehe [Deine eigene Twitch-Anwendung](settings#twitch-app).
- Kanalpunkte-Belohnungen werden nicht angesagt: Twitch bietet Kanalpunkte nur Affiliates und Partnern.
- *Die Twitch-Anmeldung ist abgelaufen*: Melde dich neu an (Twitch beendet Anmeldungen nach einer Passwortänderung oder 30 Tagen ohne Nutzung).

## Protokolle {#logs}

TTV-TTS schreibt eine Protokolldatei neben seine Einstellungen ([Datenordner](installation#data)); der Proxy zeigt sein Protokoll auf der Seite **Protokoll** (**Alles kopieren**). Häng sie an, wenn du [ein Problem meldest](https://github.com/ZcarecroW/TTV-TTS/issues).
