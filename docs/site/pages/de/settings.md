---
title: Einstellungen
description: Ton, Tempo, Tastenkürzel, Fernsteuerung, das Twitch-Konto, Plugins und Updates.
icon: Gear
---

![Die Seite Einstellungen](settings.webp)

## Ton {#sound}

- **Ausgabegerät** – wo TTV-TTS spielt. *Systemstandard* folgt deinem System. Sollen die Stimmen eine eigene OBS-Quelle sein, nimm statt eines virtuellen Kabels einen [OBS-Link](obs).
- **Lautstärke** – auch aus dem Chat mit `!ttsvol` ([Mod-Befehle](profiles#mod-commands)).
- **Pause zwischen Nachrichten** – ein kurzes Durchatmen zwischen zwei Nachrichten.
- **Stimmeffekte auf dem Stimmen-PC** (mit TTV-TTS Proxy) – der Stimmen-Host wendet die Effekte an, dieser PC spielt nur ab (siehe [Effekte](effects#where)).

## Spracherzeugung {#generation}

- **Ganze Sätze** – Voice Creator Pro erzeugt jeden Satz komplett und den nächsten, während der aktuelle spielt. Auf vielen PCs ist das insgesamt deutlich schneller.
- **Streaming** – der Ton beginnt, während Voice Creator Pro noch erzeugt: der erste Ton kommt früher, der Rest kann langsamer sein.
- **Automatisch** (empfohlen) – TTV-TTS misst beides auf deinem Stimmen-Host, sobald der Chat zwei Minuten ruhig war, und nimmt das Schnellere. **Jetzt messen** misst sofort.

Gemessen auf einer RTX 3070 mit Qwen3: ganze Sätze – erster Ton nach etwa 2 s, erzeugt mit 0,56× Echtzeit; Streaming – erster Ton nach etwa 1 s, aber erzeugt mit 4,3× Echtzeit, längere Nachrichten stocken also. Dort gewinnt *Ganze Sätze*.

## Übersetzungsdienst {#translation}

Für [Für mich übersetzen](profiles#translate):

- **E-Mail für MyMemory** (optional) – MyMemory ist kostenlos; mit einer E-Mail-Adresse ist das Tageslimit zehnmal höher.
- **DeepL-API-Schlüssel** (optional) – ein kostenloser DeepL-API-Schlüssel reicht. Er liegt im Anmeldedaten-Speicher deines Systems.

## Tastenkürzel {#hotkeys}

Globale Tastenkürzel funktionieren überall, auch wenn ein Spiel oder OBS den Fokus hat: **Nachricht überspringen**, **Pause / weiter**, **Warteschlange leeren**, **Letzte nochmal**, **Nächste freigeben** und **Nächste verwerfen** ([Freigabe](live#approval)).

![Tastenkürzel](settings-hotkeys.webp)

Klick in ein Feld und drück die Kombination (mit Strg, Alt oder Umschalt – oder eine F-Taste –, damit sie beim Tippen nicht auslöst); Esc bricht ab. Eine Kombination, die schon ein anderes Programm nutzt, lässt sich nicht belegen.

## Fernsteuerung {#remote}

**Fernsteuerung erlauben** startet auf diesem PC eine kleine API für Stream Deck, Streamer.bot, Touch Portal und andere. Jede URL macht genau eine Sache – siehe [Fernsteuerungs-API](remote-api).

![Fernsteuerung](settings-remote.webp)

## Twitch-Konto (optional) {#twitch}

Der Chat wird ohne Anmeldung gelesen. Melde dich nur mit Twitch an, um auch **Follows** und **Kanalpunkte-Belohnungen ohne Text** anzusagen ([Ansagen](profiles#announcements)).

![Twitch-Konto](settings-twitch.webp)

1. Klick auf **Mit Twitch anmelden**. Dein Browser öffnet twitch.tv/activate mit einem Code, den auch TTV-TTS zeigt.
2. Bestätige ihn bei Twitch. TTV-TTS merkt das von selbst und zeigt *Angemeldet als …* mit den Ereignissen, die es empfängt.

Dein Passwort geht nie durch TTV-TTS. Es darf nur sehen, wer dir folgt und welche Belohnungen eingelöst werden (`moderator:read:followers`, `channel:read:redemptions`). Die Anmeldung liegt im Anmeldedaten-Speicher deines Systems, wird stündlich geprüft, wie Twitch es verlangt, und **Abmelden** widerruft sie bei Twitch.

### Deine eigene Twitch-Anwendung {#twitch-app}

Zum Anmelden braucht eine App eine *Client ID*: das Namensschild einer bei Twitch registrierten Anwendung. TTV-TTS hat eine eingebaut („TTV-TTS by ZcarecroW“), normalerweise klickst du also einfach auf **Mit Twitch anmelden**. Du kannst stattdessen deine eigene verwenden – kostenlos und in zwei Minuten erledigt.

**Warum du vielleicht eine eigene willst:**

- **Du bist unabhängig.** Deine Anmeldung läuft über eine Anwendung, die nur du verwaltest. Würde die eingebaute je geändert, gesperrt oder ersetzt, funktioniert deine Anmeldung weiter.
- **Du siehst sie und kannst sie widerrufen.** Beim Anmelden zeigt Twitch den Namen *deiner* Anwendung, und unter [Twitch → Einstellungen → Verbindungen](https://www.twitch.tv/settings/connections) steht sie als deine.
- **Selbst kompilierte Versionen** (siehe [Aus dem Quellcode bauen](building)) haben keine Client ID eingebaut – dort brauchst du deine eigene.
- **Nichts wird schneller oder langsamer**: Twitch zählt seine Limits pro Nutzer, die eingebaute Anwendung zu teilen kostet dich also nichts. Bist du mit der eingebauten zufrieden, musst du nichts ändern.

**So geht's:**

1. Twitch lässt nur Konten mit **Zwei-Faktor-Authentifizierung** Anwendungen registrieren – schalte sie unter [Einstellungen → Sicherheit](https://www.twitch.tv/settings/security) ein, falls noch nicht geschehen.
2. Öffne [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) (mit deinem Twitch-Konto anmelden) und wähl **Register Your Application**.
3. **Name**: etwas Eindeutiges, zum Beispiel „TTV-TTS (dein Name)“ – vergebene Namen lehnt Twitch ab. **OAuth Redirect URLs**: `http://localhost` (TTV-TTS meldet dich mit einem Code an, den du im Browser bestätigst, diese Adresse wird also nie benutzt). **Category**: *Application Integration*. **Client Type**: **Public**. Dann **Create**.
4. Klick neben der neuen Anwendung auf **Manage** und kopier die **Client ID**. Ein Client Secret brauchst du nicht – eine öffentliche Anwendung hat keins.
5. Öffne in TTV-TTS **Einstellungen → Twitch-Konto**, klapp **Eigene Twitch-Anwendung verwenden** auf, füg die Client ID ein und speichere. Dann **Mit Twitch anmelden**.

Zurück zur eingebauten Anwendung: **Die eingebaute verwenden**. Deine Client ID ist kein Geheimnis wie ein Passwort (Twitch schickt sie bei jeder Anfrage mit), aber es gibt auch keinen Grund, sie öffentlich zu posten.

## Audio-Plugins {#plugins}

VST3- und CLAP-Effekte auf jeder Stimme – siehe [Effekte & Plugins](effects#plugins).

## Aussehen & Sprache {#look}

TTV-TTS spricht Englisch und Deutsch und folgt deinem System, solange du nichts anderes wählst. **Design**: *System*, *Dunkel* (dunkelblau) oder *Hell*.

![Das helle Design](live-light.webp)

## Updates {#updates}

**Update-Kanal** *Stabil* oder *Vorschau*, **Jetzt prüfen** und **Installieren und neu starten**, wenn ein Update da ist – siehe [Updates](installation#updates).
