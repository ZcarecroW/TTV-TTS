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

## Kostenlose Plugins zum Ausprobieren {#free-plugins}

Alle hier sind kostenlos und kommen direkt von ihren Herstellern. TTV-TTS braucht die **VST3**- oder **CLAP**-Version – manche Downloads enthalten auch VST2-, AU- oder AAX-Dateien, die du ignorieren kannst. Formate und Systeme so, wie die Hersteller sie angeben (November 2026).

| Plugin | Gut für | Formate | Systeme |
|---|---|---|---|
| [Airwindows Consolidated](https://www.airwindows.com/consolidated/) | über 400 kleine Effekte in einem Plugin: EQs, Kompressoren, Lo-Fi, Band, Hall. Open Source | CLAP, VST3 | Windows, macOS, Linux |
| [Surge XT Effects](https://surge-synthesizer.github.io/) | alle Effekte des Surge-Synths: Vocoder, Ringmodulator, Frequenzverschieber, Chorus, Hall, Verzerrung. Open Source | CLAP, VST3 | Windows, macOS, Linux |
| [TAL-Vocoder](https://tal-software.com/products/tal-vocoder) | Roboterstimmen – mit eigenem Träger-Synth, funktioniert also mit Sprache allein | CLAP, VST3 | Windows, macOS, Linux (VST3) |
| [Dragonfly Reverb](https://michaelwillis.github.io/dragonfly-reverb/) | vier Hallgeräte: Saal, Raum, Plate, frühe Reflexionen. Open Source | CLAP, VST3 | Windows, macOS, Linux |
| [Graillon 3 Free Edition](https://www.auburnsounds.com/products/Graillon.html) | Tonhöhe und Formanten verschieben (tiefes Monster, winziger Gremlin), Bitcrusher | VST3 | Windows, macOS, Linux |
| [Valhalla Supermassive](https://valhalladsp.com/shop/reverb/valhalla-supermassive/) | riesige, weltraumartige Hallräume und Echos – Raumschiffe, Träume, Götter | VST3 | Windows, macOS |
| [Protoverb](https://u-he.com/products/protoverb/) | seltsame, experimentelle Räume | VST3 | Windows, macOS, Linux (Beta) |
| [Kilohearts Essentials](https://kilohearts.com/products/kilohearts_essentials) | 34 kleine Effekte: Pitch-Shifter, Ringmodulator, Frequenzverschieber, Chorus, Flanger, Verzerrung. Kostenlos mit Konto | VST3 | Windows, macOS |
| [MFreeFXBundle](https://www.meldaproduction.com/MFreeFXBundle) | über 30 Effekte, darunter MAutoPitch (Tonhöhe und Formanten), MRingModulator, EQ, Kompressor | VST3 | Windows, macOS |
| [TDR Nova](https://www.tokyodawn.net/tdr-nova/) | dynamischer EQ – zähmt scharfe „s“-Laute als De-Esser | VST3 | Windows, macOS |
| [LSP Plugins](https://lsp-plug.in/) | Kompressoren, EQs, De-Esser, Limiter. Open Source | CLAP, VST3 | Linux (Windows-Builds auf der Seite) |

Ideen für den Anfang:

- **Ein Monster oder Riese**: Graillon, Tonhöhe 5–7 Halbtöne runter, Formanten ein wenig runter – dann ein kurzer Saal von Dragonfly.
- **Ein Roboter oder Bordcomputer**: TAL-Vocoder oder der Surge-XT-Vocoder; der eingebaute Effekt **Roboter** ist die schnelle Variante.
- **Götter und Geister**: Valhalla Supermassive mit langem Ausklang, zu 20–30 % beigemischt.
- **Klarere Sprache für alle**: TDR Nova als De-Esser plus ein sanfter Kompressor am Ende der Kette.

> [!TIP]
> Lade Plugins nur von der Seite des Herstellers (die Links oben), nie von „Gratis-Download“-Spiegelseiten – ein Plugin läuft in TTV-TTS mit deinen Rechten. Plugins, die es nur als VST2 gibt, gehen nicht.
