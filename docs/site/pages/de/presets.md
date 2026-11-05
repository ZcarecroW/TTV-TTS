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

Auch bei einer langen Warteschlange bleibt die Liste kurz: oben steht, wie viele Stimmen noch offen sind und bis wann es etwa dauert, darunter der laufende Auftrag und die nächsten; alles Weitere ist nach Sprachen zusammengeklappt. Importe und Verbesserungen laufen zuerst, dann das, was du selbst startest. Pausierte und fertige Aufträge haben eigene Bereiche mit **Alle entfernen**.

> [!TIP]
> Viel zu berechnen? Schalte in Voice Creator Pro **Low VRAM** aus und **starte es neu**. TTV-TTS hält dann genau ein Modell im Speicher: Die Spracherkennung bleibt für ihre ganze Phase geladen und ist wirklich weg, bevor das nächste Modell lädt. Auf einer RTX 3070 (8 GB) dauerte eine Prüfung 0,6 s statt 5 s, eine Stimme etwa 12 s statt 28 s, und der Chat blieb genauso schnell. Ändere die Einstellung nicht, solange Voice Creator Pro Modelle geladen hat: Der Speicher kann dann voll bleiben, und bis zum nächsten Neustart wird alles etwa zehnmal langsamer.

> [!TIP]
> Viele Presets gibt es **fertig zum Herunterladen** (**Holen**): von TTV-TTS mit Qualitätsprüfung berechnet, ein paar Megabyte, in etwa einer Minute importiert.

## Alle Presets, automatisch {#plan}

**Alle Presets berechnen** berechnet jedes eingebaute Preset in den Sprachen deiner Wahl, ohne dass du etwas tun musst: Sprache für Sprache, jede Stimme geprüft, schlechte repariert, fehlgeschlagene mit mehr Versuchen wiederholt. Beim Streamen geht der Chat weiterhin vor. Die Karte zeigt jedes Preset in jeder Sprache und wann alles fertig ist. Fertige Stimmen verlassen Voice Creator Pro und bleiben als verlustfreie Aufnahmen für [Stimmpakete](packs) erhalten. Das dauert Tage; die meisten brauchen es nicht, weil es die Presets fertig zum Herunterladen gibt.

## Benchmark {#benchmark}

Der **Benchmark** auf der Seite **Stimmen-Host** misst, was dein PC wirklich schafft, denn Voice Creator Pro verliert manchmal an Tempo:

1. wie es gerade läuft,
2. ein Neustart von Voice Creator Pro,
3. der erste Start jedes Modells, dann jedes Modell bei der Arbeit mit kurzen und langen Aufnahmen,
4. was jeder Wechsel zwischen zwei Modellen kostet,
5. ob es beim Arbeiten langsamer wird.

**Schnell** dauert etwa 5 Minuten, **Standard** etwa 12, **Gründlich** etwa 45 und vergleicht zusätzlich Low VRAM an und aus und eine leichtere Spracherkennung, mit echten Stimmen in allen Sprachen; die beste Einstellung bleibt. Nichts anderes läuft währenddessen: Der Chat wartet, das Berechnen geht danach dort weiter, wo es stand.

Aus dem Ergebnis legt TTV-TTS fest, wie viele Stimmen pro Durchgang berechnet werden, damit jedes Modell seine ganze Arbeit am Stück erledigt, und die Grenzen für den [Neustart, wenn es deutlich langsamer wird](voice-host#slow).

## Anhören {#listen}

**Stimmen** öffnet die Liste der Figuren eines Presets. Wähl eine Sprache, tipp einen eigenen Text (oder lass ihn leer für einen Beispielsatz) und klick auf ▶. Filtere nach Name oder Klang, oder zeig nur berechnete Stimmen.

![Die Stimmen eines Presets](preset-detail.webp)

### Neu entwerfen {#redo}

Eine Stimme klingt anders als ihre Beschreibung? **Neu entwerfen** – entweder *automatisch* (der beste Versuch ersetzt sie) oder *ich wähle aus 3 Versuchen*: Drei neue Entwürfe werden gemacht und geprüft, und du wählst einen (oder behältst die aktuelle Stimme). Bis dahin spricht die alte Stimme weiter.

## Qualitätsprüfung {#quality}

Die Qualitätsprüfung vergleicht jede Stimme mit ihrer Beschreibung (Geschlecht, Alter, Tempo), mit ihren anderen Sprachen und damit, wie sie einen neuen Satz spricht. Stimmen, die seltsam klingen könnten, bekommen ein Abzeichen; **Anhören und entscheiden** zeigt sie mit dem Grund (zum Beispiel *Stimme hoch für die Rolle*), der Original-Aufnahme und einem neuen Satz. **Behalten**, was für dich gut klingt, **Neu entwerfen** oder **Verbessern** (TTV-TTS versucht es zweimal und behält die neue Stimme nur, wenn sie besser ist).

TTV-TTS vermeidet, dass Stimmen doppelt berechnet werden:

- Ein Versuch, der direkt nach dem Entwerfen einen Fehler zeigt (falsche Tonlage für die Rolle, zu leise, Rauschen), bekommt bis zu drei Nachfolger, solange das Entwurfsmodell geladen ist.
- Der vielversprechendste Versuch wird zuerst geprüft; ist er sauber, muss niemand den anderen zuhören.
- Eine Stimme, die nur wegen des Testsatzes auffällt, bekommt eine zweite Meinung: Sie sagt, was ihre Aufnahme sagt. Klappt das, war der Testsatz ein Ausreißer.
- Eine Stimme, die in einer anderen Sprache anders klingt, bekommt einen Hinweis, keine Neuberechnung: Für sich genommen ist sie nicht defekt.

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
