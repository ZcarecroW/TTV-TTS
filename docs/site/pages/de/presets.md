---
title: Presets
description: Besetzungen von Stimmen – eingebaut oder eigene – einmal von Voice Creator Pro berechnet.
icon: TheatreMasks
---

Ein **Preset** ist eine Besetzung von Figuren: ein Name, ein Alter, ein Geschlecht und eine Beschreibung, wie jede klingt. Voice Creator Pro *entwirft* aus jeder Beschreibung eine Stimme – einmal pro Sprache – und TTV-TTS behält sie in seiner Bibliothek. Danach ist die Stimme sofort bereit.

![Die Seite Presets](presets.webp)

## Die eingebauten Presets {#builtin}

Zwanzig Presets mit je 240 Stimmen: Cinematic, Everyday Neighbors, Fantasy Tavern, Starship Crew, Noir City, Saturday Cartoons, High Seas, Royal Court, Midnight Horror, Arena Casters, On Air, Wild Planet, Storytime, Frontier West, Neon Streets, Olympus, Capes & Masks, Grand Stage, Quest Givers und Calm & Cozy. [Hör sie dir auf der Startseite an](index).

Jede Karte zeigt, wie viele Stimmen pro Sprache berechnet sind, wie viele seltsam klingen könnten und die Mischung aus Männern, Frauen und neutralen Stimmen unter den ersten 30.

## Stimmen berechnen {#compute}

Klick bei einem Preset auf **Berechnen**:

- **Stimmen** – 30, 60, 120 oder alle 240. Die ersten 30 sind eine gemischte Besetzung; mehr Stimmen bringen mehr Abwechslung für große Chats.
- **Sprachen** – jede Sprache wird einzeln entworfen, damit eine Stimme wirklich muttersprachlich Deutsch, Japanisch oder Spanisch spricht.
- **Versuche pro Stimme** – Voice Creator Pro entwirft jede Stimme so oft, und TTV-TTS behält den besten Versuch. Mehr Versuche, bessere Stimmen, längere Rechenzeit.
- **Sprachprüfung** – jeder Versuch spricht einen Testsatz, und TTV-TTS hört zu (Spracherkennung); Versuche, die nuscheln oder etwas anderes sagen, fliegen raus.
- **Verlustfreie Sicherung behalten** – die Original-Aufnahmen bleiben erhalten, damit die Stimmen [als Paket geteilt](packs) oder wiederhergestellt werden können.
- **Qualitätsprüfung** und **Seltsame Stimmen automatisch verbessern** – siehe [unten](#quality).

TTV-TTS zeigt, wie lange es dauert, gemessen auf deinem Stimmen-Host (**Geschwindigkeit messen**, falls noch nicht gemessen). Gerechnet wird in den Pausen zwischen Chatnachrichten – der Chat hat immer Vorrang –, du kannst also beim Streamen rechnen lassen. Mit dem Proxy läuft es auf dem anderen PC, und du kannst TTV-TTS sogar schließen.

Die Auftragsliste unter den Presets zeigt den Fortschritt. Gearbeitet wird in Stapeln: erst werden alle Versuche eines Stapels entworfen, dann geprüft, dann die besten gespeichert – deshalb springt der Zähler.

> [!TIP]
> Viele Presets gibt es **fertig zum Herunterladen** (**Holen**): von TTV-TTS mit Qualitätsprüfung berechnet, ein paar Megabyte, in etwa einer Minute importiert.

## Anhören {#listen}

**Stimmen** öffnet die Liste der Figuren eines Presets. Wähl eine Sprache, tipp einen eigenen Text (oder lass ihn leer für einen Beispielsatz) und klick auf ▶. Filtere nach Name oder Klang, oder zeig nur berechnete Stimmen.

![Die Stimmen eines Presets](preset-detail.webp)

### Neu entwerfen {#redo}

Eine Stimme klingt anders als ihre Beschreibung? **Neu entwerfen** – entweder *automatisch* (der beste Versuch ersetzt sie) oder *ich wähle aus 3 Versuchen*: Drei neue Entwürfe werden gemacht und geprüft, und du wählst einen (oder behältst die aktuelle Stimme). Bis dahin spricht die alte Stimme weiter.

## Qualitätsprüfung {#quality}

Die Qualitätsprüfung vergleicht jede Stimme mit ihrer Beschreibung (Geschlecht, Alter, Tempo), mit ihren anderen Sprachen und damit, wie sie einen neuen Satz spricht. Stimmen, die seltsam klingen könnten, bekommen ein Abzeichen; **Anhören und entscheiden** zeigt sie mit dem Grund (zum Beispiel *Stimme hoch für die Rolle*), der Original-Aufnahme und einem neuen Satz. **Behalten**, was für dich gut klingt, **Neu entwerfen** oder **Verbessern** (TTV-TTS versucht es zweimal und behält die neue Stimme nur, wenn sie besser ist).

**Stimmen prüfen** prüft, ob jede berechnete Stimme noch in Voice Creator Pro existiert, und misst die Lautstärke, damit alle Stimmen gleich laut spielen.

## Eigene Presets {#own-presets}

**Neues Preset** – zwei Wege:

![Ein neues Preset mit einer KI](preset-editor.webp)

### Mit einer KI {#ai}

1. Beschreib ein Thema („ein gemütliches Café mit Baristas, Stammgästen und einem grummeligen Besitzer“) und wie viele Stimmen du willst.
2. **Prompt kopieren** und in einen beliebigen KI-Chat einfügen (ChatGPT, Claude, Gemini …).
3. Die Antwort zurück einfügen und **Übernehmen**. Lange Presets kommen in Teilen zu 60 – wiederhol es mit dem nächsten Prompt.

TTV-TTS prüft jede Zeile und zeigt Fehler und Warnungen; Zeilen mit Fehlern bleiben beim Speichern weg.

### Tippen oder einfügen {#line-format}

Ein Preset ist reiner Text, eine Stimme pro Zeile:

```text
#ttv-tts preset v1
name: Mein Café
desc: Baristas und Stammgäste
icon: Mug
---
Mia Bloom|f|27|Bright, quick mezzo with a laugh in every sentence, warm and chatty.
Otto Grimm|m|61|Low, gravelly grumble, slow and short-tempered, sighs before he answers.|fx=telephone
```

| Feld | |
|---|---|
| Name | bis 40 Zeichen |
| Geschlecht | `m`, `f` oder `n` |
| Alter | 5 bis 99 |
| Beschreibung | 20 bis 400 Zeichen: Tonhöhe, Klangfarbe, Tempo, Haltung (die eingebauten Presets sind auf Englisch beschrieben) |
| Effekt (optional) | `fx=radio`, `telephone`, `megaphone`, `robot`, `cave`, `hall`, `walkie`, `underwater` oder `whisper` |

**Als Text kopieren** kopiert jedes Preset in diesem Format; **Zum Bearbeiten kopieren** macht eine bearbeitbare Kopie eines eingebauten.

## Stimmen aus TTV-TTS 1.x {#classic}

Hat Voice Creator Pro die Stimmen von TTV-TTS 1.x (`TTS-de 01 …`), bietet TTV-TTS an, sie **als Preset zu übernehmen** („Classic 30“) – nichts muss berechnet werden. Siehe [Von 1.x umsteigen](from-v1).
