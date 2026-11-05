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

With a long queue the list stays short: on top how many voices are left and until when, then the job at work and the next ones; everything after that is folded by language. Imports and improvements run first, then what you start yourself. Paused and finished jobs have their own sections, with **Remove all**.

> [!TIP]
> Computing a lot? Turn off **low VRAM** in Voice Creator Pro and **restart it**. TTV-TTS then keeps exactly one model in memory: speech recognition stays loaded for its whole phase and is really gone before the next model loads. On an RTX 3070 (8 GB) a check took 0.6 s instead of 5 s, a voice about 12 s instead of 28 s, and chat stayed just as fast. Don't change the setting while Voice Creator Pro has models loaded: the memory can stay full and everything gets about ten times slower until the next restart.

> [!TIP]
> Many presets can be **downloaded ready-made** (**Get**): computed by TTV-TTS with the quality check, a few megabytes, imported in about a minute.

## All presets, automatically {#plan}

**Compute all presets** computes every built-in preset in the languages you pick, without you doing anything: language by language, each voice checked, bad ones repaired, failed ones tried again with more takes. On a stream chat still goes first. The card shows every preset in every language and when it'll be done. Finished voices leave Voice Creator Pro and stay as lossless samples, ready for [voice packs](packs). This takes days; most people don't need it, because the presets can be downloaded ready-made.

## Benchmark {#benchmark}

The **Benchmark** on the **Voice host** page measures what your PC really manages, because Voice Creator Pro sometimes loses speed:

1. how it runs right now,
2. a restart of Voice Creator Pro,
3. the first start of every model, then each model at work with short and long audio,
4. what each switch between two models costs,
5. whether it gets slower as it works.

**Quick** takes about 5 minutes, **Standard** about 12, **Thorough** about 45 and also compares low VRAM on and off and a lighter speech recognition, on real voices in all languages; the best setting stays. Nothing else runs meanwhile: chat waits, computing continues afterwards where it stopped.

From the result TTV-TTS sets how many voices it computes per batch, so that each model does all its work in one go, and the limits for [restarting when it gets much slower](voice-host#slow).

## Listening {#listen}

**Voices** opens the list of a preset's characters. Choose a language, type your own text (or leave it empty for a sample sentence) and click ▶. Filter by name or sound, or show only computed voices.

![A preset's voices](preset-detail.webp)

### Design again {#redo}

A voice sounds different from its description? **Design again** – either *automatically* (the best take replaces it) or *let me pick from 3 takes*: three new designs are made and checked, and you choose one (or keep the current voice). The old voice keeps speaking until then.

## Quality check {#quality}

The quality check compares every voice with its description (gender, age, pace), with its other languages and with how it speaks a new sentence. Voices that may sound odd get a badge; **Listen and decide** shows them with the reason (for example *pitch high for the role*), the original sample and a new sentence. **Keep** what sounds fine to you, **Design again**, or **Improve** (TTV-TTS tries twice and keeps the new voice only if it's better).

TTV-TTS avoids computing voices twice:

- A take that shows a defect right after designing (wrong pitch for the role, too quiet, noise) gets up to three successors while the design model is loaded.
- The most promising take is checked first; once it's clean, the others need no listening.
- A voice listed only for how it spoke the test sentence gets a second opinion: it says what its sample says. If that's fine, the test sentence was a fluke.
- A voice that sounds different in another language gets a note, not a redo: it isn't defective by itself.

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
