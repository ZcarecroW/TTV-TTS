---
title: Effects & plugins
description: Radio, robot, cave – built-in voice effects, and your own VST3 and CLAP plugins on top.
icon: Bolt
---

Effects are applied after Voice Creator Pro has spoken, so the voice itself stays clean and every effect works with every voice.

## Where effects are applied {#where}

- **With TTV-TTS Proxy on a second PC**, the voice host PC applies the built-in effects (and each voice's loudness correction) right after generating the voice. Your gaming PC only plays what arrives. That's the default: **Settings → Sound → Voice effects on the voice host PC**. Switch it off to apply them on this PC instead.
- **With Voice Creator Pro on this PC** (or a friend's voice host), this PC applies them. They cost a few milliseconds per sentence.
- **Audio plugins** (VST3, CLAP) run on the voice host PC too, when it has them installed ([Plugins on the voice host PC](#host-plugins)); otherwise on this PC.
- **[OBS links through the voice host PC](obs#host)** do everything on the voice host PC.

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
- The plugins run on a thread of their own at your output device's sample rate (on the voice host PC at 48 kHz). Measured with two plugins: about 15 ms of work per second of speech – nothing you'd notice.

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

### Plugins on the voice host PC {#host-plugins}

With TTV-TTS Proxy and **Settings → Sound → Voice effects on the voice host PC** (the default), the voice host PC runs the plugins too, so your gaming PC only plays what arrives. TTV-TTS sends its chains there with the plugins' settings.

- **Install the plugin on both PCs** (or only on the voice host PC). Another folder or another version is fine: TTV-TTS finds a plugin by its ID.
- Each plugin shows where it runs: **voice host PC**, **this PC** (not installed on the voice host PC), or **only on the voice host PC** (installed only there – it then needs *Voice effects on the voice host PC*).
- A message's plugins all run in the same place, to keep their order: if the voice host PC lacks one of them, that message's plugins run here.
- **Edit** opens the plugin's window here; the server button opens it **on the voice host PC's screen** (for plugins installed only there). Its settings come back to TTV-TTS when you close it.
- The voice host PC looks for its plugins the first time TTV-TTS uses plugins; **Look again** after installing new ones. The proxy window shows them on its **OBS links** page.
- [OBS links through the voice host PC](obs#host) use the same plugins.
- Every reader on the voice host PC (TTV-TTS, each OBS link) has its own plugin instances, so a reverb never rings into another stream; instances a reader doesn't use for 10 minutes are unloaded.

> [!WARNING]
> Plugins run inside TTV-TTS Proxy there: a faulty plugin can crash the proxy. Try new plugins outside a stream.

## Plugin effects for viewers {#viewer-plugins}

Like their voices, viewers can get a **plugin effect** of their own at random – a pitch shifter as "Chipmunk", the same plugin again as "Giant", a vocoder as "Robot".

![Plugin effects for viewers in the settings](settings-viewer-plugins.webp)

1. **Settings → Audio plugins → Plugin effects for viewers → Add effect …**: give it a name and pick a plugin.
2. **Edit** opens the plugin's window: set it up. Each effect keeps its own settings, so one plugin can be several effects.
3. **How often** (×1 … ×5): ×2 is picked twice as often as ×1.
4. **Profiles → Voices → Plugin effects for viewers**: how many viewers get one (0 % = off).

- Who gets one is fixed per viewer (from their Twitch ID): raising the share only adds viewers, nobody loses theirs. The effect a viewer got is kept, like their voice.
- **Not on voices that have an effect of their own** (on by default): a voice that already sounds like a radio or a robot gets no plugin on top.
- On the [Viewers](viewers) page you can give one viewer a particular effect, none, or let chance decide again.
- The viewer's effect runs before the [chain on every voice](#plugins).

**What it costs**: each effect is one plugin instance, shared by every viewer who has it – twelve effects (the maximum) cost twelve plugins' memory, whether five or five thousand viewers have one. Processing takes a few milliseconds per second of speech for typical effects (measured with a simple plugin: 2–10 ms incl. resampling). In TTV-TTS the effects load when it starts and when you change them, not at a viewer's first message; on the voice host PC at a reader's first message with that effect (milliseconds to a second, depending on the plugin).

## Free plugins worth trying {#free-plugins}

All of these are free and come straight from their makers. TTV-TTS needs the **VST3** or **CLAP** version – some downloads also contain VST2, AU or AAX files, which you can ignore. Formats and systems as the makers list them (November 2026).

| Plugin | Good for | Formats | Systems |
|---|---|---|---|
| [Airwindows Consolidated](https://www.airwindows.com/consolidated/) | 400+ small effects in one plugin: EQs, compressors, lo-fi, tape, reverbs. Open source | CLAP, VST3 | Windows, macOS, Linux |
| [Surge XT Effects](https://surge-synthesizer.github.io/) | every effect of the Surge synth: vocoder, ring modulator, frequency shifter, chorus, reverb, distortion. Open source | CLAP, VST3 | Windows, macOS, Linux |
| [TAL-Vocoder](https://tal-software.com/products/tal-vocoder) | robot voices – it has its own carrier synth, so it works on speech alone | CLAP, VST3 | Windows, macOS, Linux (VST3) |
| [Dragonfly Reverb](https://michaelwillis.github.io/dragonfly-reverb/) | four reverbs: hall, room, plate, early reflections. Open source | CLAP, VST3 | Windows, macOS, Linux |
| [Graillon 3 Free Edition](https://www.auburnsounds.com/products/Graillon.html) | pitch and formant shifting (deep monster, tiny gremlin), bitcrusher | VST3 | Windows, macOS, Linux |
| [Valhalla Supermassive](https://valhalladsp.com/shop/reverb/valhalla-supermassive/) | huge, spacey reverbs and echoes – starships, dreams, gods | VST3 | Windows, macOS |
| [Protoverb](https://u-he.com/products/protoverb/) | strange, experimental rooms | VST3 | Windows, macOS, Linux (beta) |
| [Kilohearts Essentials](https://kilohearts.com/products/kilohearts_essentials) | 34 small effects: pitch shifter, ring mod, frequency shifter, chorus, flanger, distortion. Free with an account | VST3 | Windows, macOS |
| [MFreeFXBundle](https://www.meldaproduction.com/MFreeFXBundle) | 30+ effects, among them MAutoPitch (pitch and formant), MRingModulator, EQ, compressor | VST3 | Windows, macOS |
| [TDR Nova](https://www.tokyodawn.net/tdr-nova/) | dynamic EQ – tames sharp "s" sounds as a de-esser | VST3 | Windows, macOS |
| [LSP Plugins](https://lsp-plug.in/) | compressors, EQs, de-essers, limiters. Open source | CLAP, VST3 | Linux (Windows builds on the site) |

Ideas to start with:

- **A monster or giant**: Graillon, pitch down 5–7 semitones, formant down a little – then a short hall from Dragonfly.
- **A robot or ship's computer**: TAL-Vocoder or the Surge XT vocoder; the built-in **Robot** effect is the quick version.
- **Gods and spirits**: Valhalla Supermassive with a long decay, mixed in at 20–30 %.
- **Clearer speech for everyone**: TDR Nova as a de-esser plus a gentle compressor at the end of the chain.
- **Viewer effects**: Graillon or MAutoPitch as "Chipmunk" (pitch +5) and "Giant" (pitch −5), TAL-Vocoder as "Robot", a long Supermassive reverb as "Ghost".

> [!TIP]
> Download plugins only from the maker's own page (the links above), never from "free download" mirror sites – a plugin runs inside TTV-TTS with your rights. Plugins that exist only as VST2 can't be used.
