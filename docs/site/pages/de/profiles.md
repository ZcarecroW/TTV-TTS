---
title: Profile
description: Pro Streamer oder Kanal – was gelesen wird, wie es aufgeräumt wird und welche Stimmen sprechen.
icon: Users
---

Ein **Profil** enthält alles zum Vorlesen des Chats eines Streamers: Kanäle, Regeln, Stimmen, Sprachen. Die meisten brauchen eins; leg weitere für andere Kanäle oder Formate an (**Neues Profil**). Änderungen werden automatisch gespeichert.

![Die Seite Profile](profiles.webp)

## Profil und Kanäle {#basics}

**Kanäle** – ein oder mehrere Twitch-Kanalnamen. TTV-TTS liest sie anonym; keine Anmeldung nötig. Eingeschaltet wird das Profil auf der Seite [Live](live).

## Was gelesen wird {#reading}

- **Lesen** – *jede Nachricht*, *nur „!tts …“* (mit deinem eigenen Befehl) oder *nur diese Rollen* (Streamer, Mods, VIPs, Abonnenten).
- **Nur diese Kanalpunkte-Belohnung** – füg die ID einer Belohnung ein, um nur Nachrichten mit dieser Belohnung zu lesen (eine Belohnung mit Text, zum Beispiel „Text vorlesen“).
- **Eigene Nachrichten des Streamers ignorieren**, **Ignorierte Nutzer** (Bots wie Nightbot), **Nachrichten ignorieren, die beginnen mit** (`!` für Befehle).
- **Texte zu Subs und Resubs vorlesen** – die Nachricht, die ein Zuschauer zu seinem Resub schreibt.
- **Nachrichten vor dem Vorlesen freigeben** – siehe [Nachrichten freigeben](live#approval).
- **Systemstimme, wenn der Stimmen-Host ausfällt** – siehe [unten](#os-voice).

### Systemstimme als Ersatz {#os-voice}

Mit **Systemstimme, wenn der Stimmen-Host ausfällt** gehen Nachrichten nicht verloren, wenn Voice Creator Pro nicht sprechen kann (beendet, startet neu, sein PC aus oder nicht erreichbar): Die eigene Stimme deines Betriebssystems liest sie stattdessen – schlicht, aber der Chat läuft weiter. Windows nimmt eine installierte Stimme der Nachrichtensprache, macOS seine eingebaute Stimme (`say`), Linux `espeak-ng` (vorher installieren). Sobald Voice Creator Pro wieder antwortet, übernehmen dessen Stimmen.

### Grenzen {#limits}

**Max. Zeichen**, **Max. Sekunden pro Nachricht**, **Abklingzeit pro Zuschauer** und **Max. wartende Nachrichten** verhindern, dass sich ein voller Chat staut.

### Namen {#names}

**Sagen, wer schreibt** – *bei Wechsel* (nur wenn jemand anderes schreibt), *immer* oder *nie*. **Namen aufräumen** macht aus `max_123` „max“. Unter **Aussprache von Namen** legst du fest, wie ein Name gesprochen wird.

## Sprache {#language}

TTV-TTS erkennt die Sprache jeder Nachricht und liest sie in dieser Sprache mit der Stimme des Zuschauers. Die **Standardsprache** gilt, wenn eine Nachricht zu kurz ist, um es zu erkennen. **Sprache eines Zuschauers merken** behält die Sprache eines Zuschauers ein paar Minuten, damit ein kurzes „lol“ nach einem deutschen Satz noch deutsch gelesen wird. **Sprachen, die das Modell nicht spricht** legt fest, was mit Nachrichten in einer Sprache passiert, die dein Voice-Creator-Pro-Modell nicht kann.

## Für mich übersetzen {#translate}

Schreibt dein Chat in Sprachen, die du nicht sprichst, schalte **Andere Sprachen in meiner vorlesen** ein:

![Für mich übersetzen](profiles-translate.webp)

- **In** – deine Sprache (standardmäßig die des Profils).
- **Übersetzt von** – *MyMemory* (kostenlos, online), *DeepL* (dein eigener API-Schlüssel, siehe [Einstellungen](settings#translation)) oder *Voice Creator Pro* (offline, aber langsamer).
- **Sprachen, die ich verstehe** – werden nie übersetzt.
- **Vorlesen** – *die Übersetzung*, *Original, dann Übersetzung* oder *Original, Übersetzung anzeigen* (die Übersetzung erscheint nur auf der Live-Seite).

Der Zuschauer behält seine Stimme – sie spricht nur deine Sprache. Zum Testen tippst du im Feld *Probier es* eine Nachricht in einer anderen Sprache und klickst auf **Übersetzen**.

## Emotes, Emojis und Text {#cleanup}

- **Emotes** – Twitch-, BetterTTV-, FrankerFaceZ- und 7TV-Emotes (7TV-Änderungen kommen live): *je einmal*, *entfernen* oder *alle lesen*; **Namen trennen** liest `PepeLaugh` als „Pepe Laugh“; **Aussprache von Emotes** legt fest, wie eins gesprochen wird.
- **Emojis** – *als Wörter* („😂“ → „lachend“), *entfernen* oder *behalten*.
- **Links** – „Link“ sagen, entfernen oder die Nachricht überspringen. **@Erwähnungen** – den Namen lesen oder entfernen.
- **Wiederholungen kürzen** (`sooooo` → „so“), **GROSSBUCHSTABEN klein schreiben**.
- **Gesperrte Wörter** – Nachricht überspringen oder „piep“ sagen. **Ersetzungen** – deine eigene Wortliste, pro Sprache.

## Stimmen {#voices}

**Wer welche Stimme bekommt**:

| Modus | |
|---|---|
| **Preset** | jeder Zuschauer bekommt eine Stimme des gewählten [Presets](presets), in jeder Sprache dieselbe (empfohlen) |
| **Rolle pro Zuschauer** | für Stimmen-Bibliotheken mit Namen wie `TTS-de 03 …`, `TTS-en 03 …` – jeder Zuschauer behält eine Rolle, gesprochen von der Stimme dieser Rolle in der Nachrichtensprache (das Schema von 1.x) |
| **Stimme pro Zuschauer** | eine Stimme aus allen Stimmen von Voice Creator Pro pro Zuschauer |
| **Stimme pro Sprache** / **eine Stimme** | einfache Setups |

Bei *Preset* wählst du das Preset und wie viele seiner Stimmen genutzt werden (*alle* oder die ersten 30, 60 …). Die **Standardstimme** spricht, solange noch keine Preset-Stimme da ist. **Feste Stimmen für Zuschauer** legt einer Person eine Stimme fest; **Diese Stimmen nie verwenden** lässt Stimmen weg.

### Gefühle aus dem Chat {#emotions}

Mit **Gefühle aus dem Chat** machen Lach-Emotes (KEKW), Emojis (😂 😭 😱), Wörter wie „haha“, „rip“ oder „neeein“ und Geschrei die Stimme fröhlich, traurig, ängstlich oder überrascht. **Höchstens** begrenzt, wie stark es wird. Gemischte oder unklare Nachrichten bleiben neutral. Es braucht ein Modell mit Gefühlen in Voice Creator Pro (Qwen3) und macht das Sprechen nicht langsamer.

### Stimmeffekte {#effects}

**Stimmeffekte** lässt die Effekte eines Presets spielen (Radio, Telefon …) – siehe [Effekte & Plugins](effects).

## Ansagen {#announcements}

Standardmäßig aus – Chatbots sagen oft schon an. Eingeschaltet sagt TTV-TTS Ereignisse mit einer Ansager-Stimme deiner Wahl an:

![Ansagen](profiles-events.webp)

- Abos und Resubs, verschenkte Abos (20 Geschenke auf einmal sind eine Ansage), Raids (ab einer Zuschauerzahl), Cheers (ab einer Anzahl Bits) – alles ohne Twitch-Anmeldung.
- **Follows** und **Kanalpunkte-Belohnungen ohne Text** brauchen die optionale [Twitch-Anmeldung](settings#twitch). Follows sind standardmäßig aus (Follow-Bots), und höchstens alle 10 Sekunden wird ein Follow angesagt.
- **Eigene Sätze** mit Platzhaltern wie `{user}`, `{months}`, `{viewers}`, `{bits}`, `{reward}`; ▶ spielt ein Beispiel.

## Mod-Befehle {#mod-commands}

Mit **Mods dürfen TTS im Chat steuern** funktionieren diese Befehle für die Rollen, die du erlaubst (standardmäßig Streamer und Mods):

| Befehl | |
|---|---|
| `!skip` | aktuelle Nachricht überspringen |
| `!ttsclear` | Warteschlange leeren |
| `!ttspause` / `!ttsresume` | pausieren / weiter |
| `!ttsvol 60` | Lautstärke in Prozent |
| `!ttsmute name` / `!ttsunmute name` | einen Zuschauer stumm / wieder laut schalten |

Alle lassen sich umbenennen. **Timeouts, Banns und gelöschte Nachrichten verlassen die Warteschlange** – eine Nachricht, die ein Mod löscht, wird nie vorgelesen.
