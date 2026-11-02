---
title: Effekte & Plugins
description: Radio, Roboter, Höhle – eingebaute Stimmeffekte und deine eigenen VST3- und CLAP-Plugins obendrauf.
icon: Bolt
---

TTV-TTS spielt Effekte selbst ab, nachdem Voice Creator Pro gesprochen hat – die Stimme bleibt sauber, und Effekte kosten auf dem Stimmen-Host keine Zeit.

## Eingebaute Stimmeffekte {#builtin}

| Effekt | Klingt wie |
|---|---|
| **Radio** | ein altes Radio: schmal, etwas knisternd |
| **Telefon** | ein Telefonat |
| **Megafon** | ein Lautsprecher auf einer Bühne |
| **Roboter** | metallisch und künstlich |
| **Höhle** | ein langes, dunkles Echo |
| **Halle** | ein großer Raum |
| **Funkgerät** | ein Handfunkgerät |
| **Unter Wasser** | dumpf und wabernd |
| **Flüstern** | hauchig und leise |

Die Effekte sind in der Lautstärke angeglichen, damit ein Roboter nicht lauter ist als ein Flüstern.

### Wer einen Effekt bekommt {#who}

- **Im Preset**: Eine Stimmzeile kann einen Effekt tragen (`fx=radio`, siehe [Zeilenformat](presets#line-format)). Schreibst du ein [Preset mit einer KI](presets#ai), lässt **Stimmeffekte erlauben** die KI ein paar Figuren einen geben – dem Bordcomputer eines Raumschiffs, einem Piloten über Funk.
- **Pro Zuschauer**: Auf der Seite [Zuschauer](viewers) kann jeder Zuschauer einen eigenen Effekt bekommen, oder keinen.
- **Pro Profil**: **Stimmeffekte** im [Profil](profiles#effects) schaltet sie alle ein oder aus.

## Audio-Plugins (VST3, CLAP) {#plugins}

Unter **Einstellungen → Audio-Plugins** kannst du Effekt-Plugins auf jede Stimme legen – einen EQ, einen De-Esser, einen Kompressor, einen Hall – genau wie in einer DAW.

![Audio-Plugins in den Einstellungen](settings-plugins.webp)

- **Plugin hinzufügen …** listet die Effekte in den üblichen Plugin-Ordnern (jede Datei wird einmal geladen, um zu erfahren, was sie ist), oder **Aus einer Datei …** für eine `.vst3` oder `.clap` irgendwo.
- Die Kette läuft von oben nach unten, nach den eingebauten Effekten. Mit den Pfeilen änderst du die Reihenfolge, mit dem Schalter umgehst du eins, mit dem Papierkorb entfernst du es.
- **Bearbeiten** öffnet das eigene Fenster des Plugins (Windows). Seine Einstellungen werden beim Schließen gespeichert und sind nach einem Neustart wieder da.
- Die Plugins laufen auf einem eigenen Thread in der Abtastrate deines Ausgabegeräts. Gemessen mit zwei Plugins: etwa 15 ms Arbeit pro Sekunde Sprache – nichts, was du merkst.

> [!WARNING]
> Plugins laufen in TTV-TTS. Ein fehlerhaftes Plugin kann es abstürzen lassen – füg eins nach dem anderen hinzu und teste vor einem Stream. Instrumente und VST2-Plugins gehen nicht.

TTV-TTS sucht VST3-Plugins in den üblichen VST3-Ordnern deines Systems (unter Windows `C:\Program Files\Common Files\VST3`) und CLAP-Plugins in:

| | CLAP |
|---|---|
| Windows | `C:\Program Files\Common Files\CLAP`, `%LOCALAPPDATA%\Programs\Common\CLAP` |
| macOS | `/Library/Audio/Plug-Ins/CLAP`, `~/Library/Audio/Plug-Ins/CLAP` |
| Linux | `~/.clap`, `/usr/lib/clap` |

sowie in den Ordnern aus der Umgebungsvariable `CLAP_PATH`.

Plugin-Fenster öffnen sich vorerst nur unter Windows. Unter macOS und Linux klingen die Plugins genauso, mit den Einstellungen, die sie beim Hinzufügen hatten.
