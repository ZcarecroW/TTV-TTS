---
title: Presets
description: Casts of voices – built in or your own – computed once by Voice Creator Pro.
icon: TheatreMasks
---

A **preset** is a cast of characters: a name, an age, a gender and a description of how each one sounds. Voice Creator Pro *designs* a voice from each description – once per language – and TTV-TTS keeps it in its library. After that the voice is ready instantly.

![The Presets page](presets.webp)

## The built-in presets {#builtin}

Twenty presets with 240 voices each: Cinematic, Everyday Neighbors, Fantasy Tavern, Starship Crew, Noir City, Saturday Cartoons, High Seas, Royal Court, Midnight Horror, Arena Casters, On Air, Wild Planet, Storytime, Frontier West, Neon Streets, Olympus, Capes & Masks, Grand Stage, Quest Givers and Calm & Cozy. [Hear them on the home page](index).

Each card shows how many voices are computed per language, how many may sound odd, and the mix of men, women and neutral voices among the first 30.

## Computing voices {#compute}

Click **Compute** on a preset:

- **Voices** – 30, 60, 120 or all 240. The first 30 are a mixed cast; more voices mean more variety for big chats.
- **Languages** – each language is designed separately, so a voice really speaks German, Japanese or Spanish natively.
- **Takes per voice** – Voice Creator Pro designs each voice this often and TTV-TTS keeps the best take. More takes, better voices, longer computing.
- **Speech check** – each take speaks a test sentence and TTV-TTS listens to it (speech recognition); takes that mumble or say something else are dropped.
- **Keep a lossless backup** – the original samples are kept, so the voices can be [shared as a pack](packs) or restored.
- **Quality check** and **Improve odd voices automatically** – see [below](#quality).

TTV-TTS shows how long it will take, measured on your voice host (**Measure speed** if it hasn't measured yet). The work runs in pauses between chat messages – chat always goes first – so you can compute while you stream. With the proxy it runs on the other PC, and you can even close TTV-TTS meanwhile.

The job list under the presets shows the progress. It works in batches: first all takes of a batch are designed, then checked, then the best ones saved – that's why the counter grows in jumps.

> [!TIP]
> Many presets can be **downloaded ready-made** (**Get**): computed by TTV-TTS with the quality check, a few megabytes, imported in about a minute.

## Listening {#listen}

**Voices** opens the list of a preset's characters. Choose a language, type your own text (or leave it empty for a sample sentence) and click ▶. Filter by name or sound, or show only computed voices.

![A preset's voices](preset-detail.webp)

### Design again {#redo}

A voice sounds different from its description? **Design again** – either *automatically* (the best take replaces it) or *let me pick from 3 takes*: three new designs are made and checked, and you choose one (or keep the current voice). The old voice keeps speaking until then.

## Quality check {#quality}

The quality check compares every voice with its description (gender, age, pace), with its other languages and with how it speaks a new sentence. Voices that may sound odd get a badge; **Listen and decide** shows them with the reason (for example *pitch high for the role*), the original sample and a new sentence. **Keep** what sounds fine to you, **Design again**, or **Improve** (TTV-TTS tries twice and keeps the new voice only if it's better).

**Check voices** checks that every computed voice still exists in Voice Creator Pro and measures loudness, so all voices play at the same volume.

## Own presets {#own-presets}

**New preset** – two ways:

![A new preset with an AI](preset-editor.webp)

### With an AI {#ai}

1. Describe a theme (“a cozy café with baristas, regulars and a grumpy owner”) and how many voices you want.
2. **Copy prompt** and paste it into any AI chat (ChatGPT, Claude, Gemini …).
3. Paste the answer back and **Add** it. Long presets come in parts of 60 – repeat with the next prompt.

TTV-TTS checks every line and shows errors and warnings; lines with errors are left out when you save.

### Type or paste {#line-format}

A preset is plain text, one voice per line:

```text
#ttv-tts preset v1
name: My café
desc: Baristas and regulars
icon: Mug
---
Mia Bloom|f|27|Bright, quick mezzo with a laugh in every sentence, warm and chatty.
Otto Grimm|m|61|Low, gravelly grumble, slow and short-tempered, sighs before he answers.|fx=telephone
```

| Field | |
|---|---|
| Name | up to 40 characters |
| Gender | `m`, `f` or `n` |
| Age | 5 to 99 |
| Description | 20 to 400 characters: pitch, timbre, pace, attitude |
| Effect (optional) | `fx=radio`, `telephone`, `megaphone`, `robot`, `cave`, `hall`, `walkie`, `underwater` or `whisper` |

**Copy as text** copies any preset in this format; **Duplicate to edit** makes an editable copy of a built-in one.

## Voices from TTV-TTS 1.x {#classic}

If Voice Creator Pro has the voices of TTV-TTS 1.x (`TTS-de 01 …`), TTV-TTS offers to **adopt them as a preset** (“Classic 30”) – nothing has to be computed. See [Coming from 1.x](from-v1).
