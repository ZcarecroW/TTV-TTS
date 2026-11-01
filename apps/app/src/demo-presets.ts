// Demo backend: presets, computing jobs and the library. Used by demo.ts.
// Reads the real built-in preset files; jobs progress on a timer.
import type { CatalogPack, EngineStatus, Issue, JobKind, JobView, Parsed, Preset, PresetDetail, PresetView, Quality, ReviewItem, VoiceDef } from "./api";

const files = import.meta.glob("../src-tauri/presets/*.txt", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

function parse(text: string): Parsed {
  const p: Parsed = { name: "", description: "", icon: "", defs: [], issues: [] };
  let header = true;
  text.split(/\r?\n/).forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("```") || line.startsWith("#")) return;
    if (line === "---") return void (header = false);
    if (!line.includes("|")) {
      const m = header && line.match(/^(name|desc|icon):\s*(.*)$/);
      if (m) return void (m[1] === "name" ? (p.name = m[2]!) : m[1] === "desc" ? (p.description = m[2]!) : (p.icon = m[2]!));
      return void p.issues.push({ line: i + 1, level: "warning", message: "not a voice line (expected Name|gender|age|description) – skipped" });
    }
    header = false;
    const [name, g, age, description, fx] = line.replace(/^\d+[.)]\s*/, "").split("|").map((s) => s.trim());
    const gender = ({ m: "m", male: "m", man: "m", f: "f", female: "f", woman: "f", w: "f", n: "n", x: "n", nb: "n", neutral: "n", androgynous: "n" } as Record<string, VoiceDef["gender"]>)[(g ?? "").toLowerCase()];
    const a = Number((age ?? "").replace(/\D/g, ""));
    const issue = (message: string): Issue => ({ line: i + 1, level: "error", message });
    if (!gender) return void p.issues.push(issue(`gender "${g}" – use m, f or n`));
    if (!(a >= 5 && a <= 99)) return void p.issues.push(issue(`age "${age}" – use a number from 5 to 99`));
    if (!description || description.length < 20) return void p.issues.push(issue("description too short – describe pitch, timbre, pace and attitude"));
    p.defs.push({ name: name!, gender, age: a, description, fx: fx?.replace("fx=", "") || null });
  });
  if (!p.defs.length) p.issues.push({ line: 0, level: "error", message: "no voices found" });
  return p;
}

// Same order as `builtin![…]` in presets.rs; files not listed come last.
const ORDER = ["cinematic", "everyday-neighbors", "fantasy-tavern", "starship-crew", "noir-city", "saturday-cartoons", "high-seas", "royal-court", "midnight-horror", "arena-casters", "on-air", "wild-planet", "storytime", "frontier-west", "neon-streets", "olympus", "capes-and-masks", "grand-stage", "quest-givers", "calm-and-cozy"];
const rank = (id: string) => (ORDER.indexOf(id) + 1 || 999);
const all: Preset[] = Object.entries(files)
  .map(([path, text]) => {
    const p = parse(text);
    return { id: path.split("/").pop()!.replace(".txt", ""), name: p.name, description: p.description, icon: p.icon || "Waveform", builtin: true, target: p.defs.length, defs: p.defs };
  })
  .sort((a, b) => rank(a.id) - rank(b.id));

// The demo library: "def|lang" of computed voices. Cinematic has some already.
const hash = (d: VoiceDef) => `${d.gender}|${d.age}|${d.description}`;
/** "let me pick": takes waiting for a choice. */
const waitingTakes = new Map<string, import("./api").TakeChoice>();
const library = new Set<string>();
const cinematic = all.find((p) => p.id === "cinematic");
cinematic?.defs.slice(0, 30).forEach((d) => ["de", "en"].forEach((l) => library.add(`${hash(d)}:${l}`)));
cinematic?.defs.slice(30, 41).forEach((d) => library.add(`${hash(d)}:de`));

// Quality reports: the first 30 Cinematic voices were checked, four of them sound odd.
const quality = new Map<string, Quality>();
const report = (score: number, issues: Quality["issues"] = [], f0 = 140): Quality => ({ score, issues, f0, checked: Math.floor(Date.now() / 1000) - 3600, new_text: true, kept: false });
cinematic?.defs.slice(0, 30).forEach((d) => ["de", "en"].forEach((l) => quality.set(`${hash(d)}:${l}`, report(0.94))));
const oddOnes: [number, string, Quality][] = [
  [2, "en", report(0.1, [{ code: "pitch_high", value: 375, weight: 0.9, extra: null }], 375)],
  [2, "de", report(0.1, [{ code: "pitch_high", value: 400, weight: 0.9, extra: null }], 400)],
  [1, "en", report(0.45, [{ code: "pitch_low", value: 98, weight: 0.55, extra: null }], 98)],
  [19, "de", report(0.09, [{ code: "pitch_high", value: 375, weight: 0.9, extra: null }, { code: "languages_differ", value: 255, weight: 0.13, extra: "en" }], 375)],
];
for (const [i, l, q] of oddOnes) if (cinematic?.defs[i]) quality.set(`${hash(cinematic.defs[i])}:${l}`, q);
const isOdd = (k: string) => { const q = quality.get(k); return !!q && q.score < 0.75 && !q.kept; };
const langsOf = (d: VoiceDef) => [...library].filter((k) => k.startsWith(`${hash(d)}:`)).map((k) => k.split(":").pop()!);

function view(p: Preset): PresetView {
  const computed: Record<string, number> = {};
  let ready = 0;
  let odd = 0;
  for (const d of p.defs) {
    const langs = langsOf(d);
    if (langs.length) ready++;
    for (const l of langs) {
      computed[l] = (computed[l] ?? 0) + 1;
      if (isOdd(`${hash(d)}:${l}`)) odd++;
    }
  }
  const first = p.defs.slice(0, 30);
  const n = (g: string) => first.filter((d) => d.gender === g).length;
  return { id: p.id, name: p.name, description: p.description, icon: p.icon, builtin: p.builtin, count: p.defs.length, target: p.target, mix: [n("m"), n("f"), n("n")], computed, ready, odd };
}

function detail(p: Preset): PresetDetail {
  return {
    ...view(p),
    voices: p.defs.map((d, index) => {
      const langs = langsOf(d).sort();
      return { ...d, index, hash: hash(d), langs, odd: langs.filter((l) => isOdd(`${hash(d)}:${l}`)) };
    }),
  };
}

/** Where a library key's definition lives (preset, index). */
function place(key: string): [Preset, number] | null {
  const h = key.slice(0, key.lastIndexOf(":"));
  for (const p of all) {
    const i = p.defs.findIndex((d) => hash(d) === h);
    if (i >= 0) return [p, i];
  }
  return null;
}

function job(kind: JobKind, p: Preset, languages: string[], total: number, options: JobView["options"]): JobView {
  return { id: `job-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`, preset_id: p.id, preset_name: p.name, languages, options, kind, total, done: 0, failed: 0, flagged: 0, improved: 0, state: "queued", phase: "", current: null, batch: 0, step: 0, steps: 0, created: Math.floor(Date.now() / 1000), finished: null, secs_per_voice: 10.8, eta_secs: Math.round(total * 10.8), error: null };
}
/** Keys an improve or review job still has to do. */
const pending = new Map<string, string[]>();

const engine: EngineStatus = {
  jobs: [
    {
      id: "job-demo1",
      preset_id: "cinematic",
      preset_name: "Cinematic",
      languages: ["de", "en"],
      options: { speech_check: true, takes: 2, backup: true, quality: true },
      kind: "compute",
      total: 60,
      done: 60,
      failed: 0,
      flagged: 4,
      improved: 0,
      state: "done",
      phase: "",
      current: null,
      batch: 0,
      step: 0,
      steps: 0,
      created: Math.floor(Date.now() / 1000) - 7200,
      finished: Math.floor(Date.now() / 1000) - 5400,
      secs_per_voice: 10.8,
      eta_secs: null,
      error: null,
    },
  ],
  secs_per_voice: 10.8,
  library_size: library.size,
  library_rev: 1,
};

type Emit = (event: string, payload: unknown) => void;
let timer = 0;

function tick(emit: Emit) {
  const job = engine.jobs.find((j) => j.state === "running" || j.state === "queued");
  if (!job) return void (clearInterval(timer), (timer = 0));
  const keys = pending.get(job.id);
  if (keys) {
    // improve / review: one voice per tick
    job.state = "running";
    // like the engine: a batch of voices per step
    const batch = keys.splice(0, 32);
    const k = batch[batch.length - 1];
    for (const x of batch.slice(0, -1)) {
      if (job.kind === "improve") (quality.set(x, report(0.91, [], 120)), job.improved++);
      else if (job.kind === "import") library.add(x);
      else if (!quality.has(x)) quality.set(x, report(0.93));
      job.done++;
    }
    if (!k) {
      Object.assign(job, { state: "done", phase: "", current: null, finished: Math.floor(Date.now() / 1000), eta_secs: null });
      pending.delete(job.id);
    } else {
      const at = place(k);
      job.current = at ? `${at[0].defs[at[1]]!.name} · ${k.split(":").pop()}` : k;
      job.phase = job.kind === "review" ? "review" : job.kind === "import" ? "import" : "design";
      if (job.kind === "improve") {
        quality.set(k, report(0.91, [], 120));
        job.improved++;
      } else if (job.kind === "import") library.add(k);
      else if (!quality.has(k)) quality.set(k, report(0.93));
      job.done++;
      emit("app://library", { rev: ++engine.library_rev, size: library.size });
    }
    return void emit("app://job", structuredClone(job));
  }
  const p = all.find((x) => x.id === job.preset_id);
  const todo = p?.defs.flatMap((d) => job.languages.map((l) => [d, l] as const)).filter(([d, l]) => !library.has(`${hash(d)}:${l}`)) ?? [];
  job.state = "running";
  const next = todo[0];
  if (!next || job.done >= job.total) {
    Object.assign(job, { state: "done", phase: "", current: null, finished: Math.floor(Date.now() / 1000), eta_secs: null });
  } else {
    library.add(`${hash(next[0])}:${next[1]}`);
    if (job.options.quality) quality.set(`${hash(next[0])}:${next[1]}`, report(0.92));
    job.done++;
    job.phase = ["design", "check", "save", "prime"][job.done % 4]!;
    Object.assign(job, { batch: 16, step: job.done % 16, steps: 16 });
    job.current = `${next[0].name} · ${next[1]}`;
    job.eta_secs = Math.round((job.total - job.done) * 10.8);
    engine.library_size = library.size;
    emit("app://library", { rev: ++engine.library_rev, size: library.size });
  }
  emit("app://job", structuredClone(job));
}

export function presetCommand(cmd: string, args: Record<string, unknown>, emit: Emit): { value: unknown } | undefined {
  const find = (id: unknown) => {
    const p = all.find((x) => x.id === id);
    if (!p) throw "This preset doesn't exist (any more).";
    return p;
  };
  switch (cmd) {
    case "list_presets":
      return { value: all.map(view) };
    case "get_preset":
      return { value: detail(find(args.id)) };
    case "save_preset": {
      const p = structuredClone(args.preset as Preset);
      if (!p.id) p.id = `u-${Math.random().toString(36).slice(2, 10)}`;
      p.builtin = false;
      p.icon ||= "Waveform";
      const i = all.findIndex((x) => x.id === p.id);
      if (i >= 0) all[i] = p;
      else all.unshift(p);
      return { value: view(p) };
    }
    case "duplicate_preset": {
      const p = { ...structuredClone(find(args.id)), id: `u-${Math.random().toString(36).slice(2, 10)}`, builtin: false };
      p.name = `${p.name} (copy)`;
      all.unshift(p);
      return { value: view(p) };
    }
    case "delete_preset":
      all.splice(all.findIndex((x) => x.id === args.id && !x.builtin), 1);
      return { value: undefined };
    case "parse_preset":
      return { value: parse(String(args.text)) };
    case "export_preset": {
      const p = find(args.id);
      return { value: `#ttv-tts preset v1\nname: ${p.name}\ndesc: ${p.description}\n---\n${p.defs.map((d) => `${d.name}|${d.gender}|${d.age}|${d.description}${d.fx ? `|fx=${d.fx}` : ""}`).join("\n")}\n` };
    }
    case "preset_prompt": {
      const r = args.request as { theme: string; total: number; have: VoiceDef[] };
      const part = Math.floor(r.have.length / 60) + 1;
      return { value: `You design voices for a text-to-speech app that reads Twitch chat aloud. …\n\nTheme: ${r.theme}\n\n(demo – part ${part} of ${Math.ceil(r.total / 60)}; the app builds the full prompt)` };
    }
    case "engine_status":
      return { value: structuredClone(engine) };
    case "estimate":
      return { value: Math.round(Number(args.voices) * (6 + 2.4 * Number(args.takes)) * (args.check ? 1 : 0.85)) };
    case "benchmark":
      return { value: 4.1 };
    case "compute": {
      const r = args.request as { preset_id: string; count: number; languages: string[]; options: JobView["options"] };
      const p = find(r.preset_id);
      const defs = r.count ? p.defs.slice(0, r.count) : p.defs;
      const total = defs.reduce((n, d) => n + r.languages.filter((l) => !library.has(`${hash(d)}:${l}`)).length, 0);
      const j = job("compute", p, r.languages, total, r.options);
      engine.jobs.push(j);
      if (!timer) timer = window.setInterval(() => tick(emit), 700);
      return { value: structuredClone(j) };
    }
    case "job_control": {
      const j = engine.jobs.find((x) => x.id === args.id);
      if (!j) throw "This job doesn't exist (any more).";
      if (args.action === "remove") engine.jobs.splice(engine.jobs.indexOf(j), 1);
      else {
        j.state = ({ pause: "paused", resume: "queued", cancel: "cancelled" } as const)[args.action as "pause" | "resume" | "cancel"];
        if (j.state === "queued" && !timer) timer = window.setInterval(() => tick(emit), 700);
        emit("app://job", structuredClone(j));
      }
      return { value: undefined };
    }
    case "redo_voice": {
      const p = find(args.presetId);
      const def = p.defs[Number(args.index)];
      if (!def) throw "This voice doesn't exist (any more).";
      if (args.pick) {
        const key = `${hash(def)}:${args.lang}`;
        // three takes after a moment, the old voice stays
        window.setTimeout(() => {
          waitingTakes.set(key, {
            key, name: def.name, lang: String(args.lang), preset_id: p.id, index: Number(args.index), created: Math.floor(Date.now() / 1000),
            takes: [0, 1, 2].map((n) => ({ n, duration: 5.2 + n * 0.7, wer: 0, score: [0.82, 0.93, 0.71][n]!, recommended: n === 1 })),
          });
          emit("app://library", { rev: ++engine.library_rev, size: library.size });
        }, 900);
        return { value: { ...structuredClone(engine.jobs[0] ?? {}), id: "pick" + Date.now(), preset_id: p.id, preset_name: p.name, kind: "improve", total: 1, done: 0, state: "running" } };
      }
      library.delete(`${hash(def)}:${args.lang}`);
      quality.delete(`${hash(def)}:${args.lang}`);
      return presetCommand("compute", { request: { preset_id: p.id, count: Number(args.index) + 1, languages: [args.lang], options: { speech_check: true, takes: 2, backup: true, quality: true } } }, emit);
    }
    case "list_takes":
      return { value: [...waitingTakes.values()] };
    case "play_take":
      return { value: 1500 };
    case "choose_take":
    case "discard_takes":
      waitingTakes.delete(String(args.key));
      emit("app://library", { rev: ++engine.library_rev, size: library.size });
      return { value: undefined };
    case "quality_list": {
      const items: ReviewItem[] = [];
      for (const k of library) {
        if (!isOdd(k)) continue;
        const at = place(k);
        if (!at) continue;
        const d = at[0].defs[at[1]]!;
        items.push({ key: k, preset_id: at[0].id, preset_name: at[0].name, index: at[1], name: d.name, gender: d.gender, age: d.age, description: d.description, lang: k.split(":").pop()!, quality: structuredClone(quality.get(k)!) });
      }
      items.sort((a, b) => a.quality.score - b.quality.score);
      const checked = [...library].filter((k) => quality.has(k)).length;
      return { value: { items, checked, unchecked: library.size - checked, kept: [...quality.values()].filter((q) => q.kept && q.score < 0.75).length } };
    }
    case "keep_voices": {
      let n = 0;
      for (const k of args.keys as string[]) {
        const q = quality.get(k);
        if (q && q.kept !== args.kept) (q.kept = Boolean(args.kept)), n++;
      }
      emit("app://library", { rev: ++engine.library_rev, size: library.size });
      return { value: n };
    }
    case "improve_voices":
    case "check_quality": {
      const improve = cmd === "improve_voices";
      const keys = improve ? (args.keys as string[]) : [...library].filter((k) => (args.again || !quality.has(k)) && (!args.presetId || place(k)?.[0].id === args.presetId));
      if (!keys.length) return { value: [] };
      const first = place(keys[0]!);
      const j = job(improve ? "improve" : "review", first ? first[0] : all[0]!, [...new Set(keys.map((k) => k.split(":").pop()!))], keys.length, { speech_check: true, takes: 2, backup: true, quality: true });
      pending.set(j.id, [...keys]);
      engine.jobs.push(j);
      if (!timer) timer = window.setInterval(() => tick(emit), 700);
      return { value: [structuredClone(j)] };
    }
    case "pack_catalog": {
      const c: CatalogPack[] = [];
      for (const id of ["fantasy-tavern", "starship-crew"]) for (const lang of ["de", "en"]) c.push({ file: `${id}-${lang}-1-30.ttvpack`, preset: id, lang, from: 1, to: 30, voices: 30, bytes: 5_900_000, sha256: "0".repeat(64) });
      return { value: { version: 1, updated: "2026-11-01", packs: c } };
    }
    case "download_packs": {
      const out: JobView[] = [];
      for (const f of args.files as string[]) {
        const [id, lang] = [f.split("-").slice(0, -3).join("-"), f.split("-").slice(-3)[0]!];
        const p = find(id);
        const keys = p.defs.slice(0, 30).map((d) => `${hash(d)}:${lang}`).filter((k) => !library.has(k));
        const j = job("import", p, [lang], keys.length, { speech_check: false, takes: 1, backup: true });
        pending.set(j.id, keys);
        engine.jobs.push(j);
        out.push(structuredClone(j));
      }
      if (!timer) timer = window.setInterval(() => tick(emit), 700);
      return { value: new Promise((res) => setTimeout(() => res(out), 600)) };
    }
    case "pick_pack":
      return { value: "C:\\Users\\demo\\Downloads\\Saturday Cartoons de en.ttvpack" };
    case "pack_info": {
      const p = find("saturday-cartoons");
      const keys = p.defs.slice(0, 30).flatMap((d) => ["de", "en"].map((l) => `${hash(d)}:${l}`));
      return { value: { path: String(args.path), title: `${p.name} – 60 voices (de, en)`, created: Math.floor(Date.now() / 1000) - 86400, app: "2.0.0-alpha.10", bytes: 11_800_000, voices: keys.length, new_voices: keys.filter((k) => !library.has(k)).length, languages: ["de", "en"], presets: [{ id: p.id, name: p.name, builtin: true, voices: keys.length, status: "known" }] } };
    }
    case "import_pack": {
      const p = find("saturday-cartoons");
      const keys = p.defs.slice(0, 30).flatMap((d) => ["de", "en"].map((l) => `${hash(d)}:${l}`)).filter((k) => !library.has(k));
      const j = job("import", p, ["de", "en"], keys.length, { speech_check: false, takes: 1, backup: true });
      j.preset_name = `${p.name} – 60 voices (de, en)`;
      pending.set(j.id, [...keys]);
      engine.jobs.push(j);
      if (!timer) timer = window.setInterval(() => tick(emit), 700);
      return { value: { job: structuredClone(j), presets: [] } };
    }
    case "export_pack": {
      const p = find(args.presetId);
      const langs = args.languages as string[];
      const n = (args.keys as string[]).length || p.defs.slice(0, Number(args.count) || p.defs.length).reduce((s, d) => s + langs.filter((l) => library.has(`${hash(d)}:${l}`)).length, 0);
      if (!n) throw "None of these voices is computed yet.";
      return { value: new Promise((res) => setTimeout(() => res({ path: `C:\\Users\\demo\\Documents\\${p.name} ${langs.join(" ")}.ttvpack`, voices: n, bytes: n * 196_000 }), 800)) };
    }
    case "play_reference":
      return { value: new Promise((res) => setTimeout(() => res(7.2), 1200)) };
    case "verify_library":
      return { value: new Promise((res) => setTimeout(() => res({ checked: library.size, removed: 0, measured: 90 }), 600)) };
    case "listen":
      return { value: new Promise((res) => setTimeout(() => res({ pieces: 1, first_audio_ms: 1120, total_ms: 3900, audio_secs: 3.1, rtf: 0.52 }), 1500)) };
    case "detect_classic":
      return { value: { roles: 30, voices: 300, languages: ["de", "en", "es", "fr", "it", "ja", "ko", "pt", "ru", "zh"] } };
    case "adopt_classic":
      all.unshift({ id: "classic", name: "Classic 30", description: "Your voices from TTV-TTS 1.x", icon: "ClockRotate", builtin: false, target: 30, defs: (cinematic?.defs ?? []).slice(200, 230) });
      (cinematic?.defs ?? []).slice(200, 230).forEach((d) => ["de", "en", "ru"].forEach((l) => library.add(`${hash(d)}:${l}`)));
      return { value: { roles: 30, adopted: 90 } };
  }
  return undefined;
}
