---
title: Effects & plugins
description: Radio, robot, cave – built-in voice effects, and your own VST3 and CLAP plugins on top.
icon: Bolt
---

TTV-TTS plays effects itself, after Voice Creator Pro has spoken – the voice stays clean, and effects cost no time on the voice host.

## Built-in voice effects {#builtin}

| Effect | Sounds like |
|---|---|
| **Radio** | an old radio: narrow, a little crackly |
| **Telephone** | a phone call |
| **Megaphone** | a loudspeaker on a stage |
| **Robot** | metallic and synthetic |
| **Cave** | a long, dark echo |
| **Hall** | a big room |
| **Walkie-talkie** | a handheld radio set |
| **Underwater** | muffled and wobbly |
| **Whisper** | breathy and quiet |

Effects are evened out in loudness, so a robot isn't louder than a whisper.

### Who gets an effect {#who}

- **In a preset**: a voice line can carry an effect (`fx=radio`, see [line format](presets#line-format)). When you [write a preset with an AI](presets#ai), **Allow voice effects** lets the AI give a few characters one – a starship's on-board computer, a pilot over the radio.
- **Per viewer**: on the [Viewers](viewers) page any viewer can get an effect of their own, or none.
- **Per profile**: **Voice effects** in the [profile](profiles#effects) switches them all on or off.

## Audio plugins (VST3, CLAP) {#plugins}

Under **Settings → Audio plugins** you can put effect plugins on every voice – an EQ, a de-esser, a compressor, a reverb – exactly like in a DAW.

![Audio plugins in the settings](settings-plugins.webp)

- **Add plugin …** lists the effects in the standard plugin folders (each file is loaded once to find out what it is), or **From a file …** for a `.vst3` or `.clap` anywhere.
- The chain runs top to bottom, after the built-in effects. Use the arrows to reorder, the switch to bypass one, the bin to remove it.
- **Edit** opens the plugin's own window (Windows). Its settings are saved when you close it and come back after a restart.
- The plugins run on a thread of their own at your output device's sample rate. Measured with two plugins: about 15 ms of work per second of speech – nothing you'd notice.

> [!WARNING]
> Plugins run inside TTV-TTS. A faulty plugin can crash it – add one at a time and test before a stream. Instruments and VST2 plugins can't be used.

TTV-TTS looks for VST3 plugins in your system's standard VST3 folders (on Windows `C:\Program Files\Common Files\VST3`), and for CLAP plugins in:

| | CLAP |
|---|---|
| Windows | `C:\Program Files\Common Files\CLAP`, `%LOCALAPPDATA%\Programs\Common\CLAP` |
| macOS | `/Library/Audio/Plug-Ins/CLAP`, `~/Library/Audio/Plug-Ins/CLAP` |
| Linux | `~/.clap`, `/usr/lib/clap` |

and in the folders listed in the `CLAP_PATH` environment variable.

Plugin windows open on Windows only for now. On macOS and Linux the plugins sound the same, with the settings they had when you added them.
