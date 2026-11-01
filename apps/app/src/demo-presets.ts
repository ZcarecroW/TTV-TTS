// Demo backend: presets, computing jobs and the library. Used by demo.ts.
// Reads the real built-in preset files; jobs progress on a timer.
import type { EngineStatus, Issue, JobView, Parsed, Preset, PresetDetail, PresetView, VoiceDef } from "./api";

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

const all: Preset[] = Object.entries(files).map(([path, text]) => {
  const p = parse(text);
  return { id: path.split("/").pop()!.replace(".txt", ""), name: p.name, description: p.description, icon: p.icon || "Waveform", builtin: true, target: p.defs.length, defs: p.defs };
});

// The demo library: "def|lang" of computed voices. Cinematic has some already.
const hash = (d: VoiceDef) => `${d.gender}|${d.age}|${d.description}`;
const library = new Set<string>();
const cinematic = all.find((p) => p.id === "cinematic");
cinematic?.defs.slice(0, 30).forEach((d) => ["de", "en"].forEach((l) => library.add(`${hash(d)}:${l}`)));
cinematic?.defs.slice(30, 41).forEach((d) => library.add(`${hash(d)}:de`));

function view(p: Preset): PresetView {
  const computed: Record<string, number> = {};
  let ready = 0;
  for (const d of p.defs) {
    const langs = [...library].filter((k) => k.startsWith(`${hash(d)}:`)).map((k) => k.split(":").pop()!);
    if (langs.length) ready++;
    for (const l of langs) computed[l] = (computed[l] ?? 0) + 1;
  }
  const first = p.defs.slice(0, 30);
  const n = (g: string) => first.filter((d) => d.gender === g).length;
  return { id: p.id, name: p.name, description: p.description, icon: p.icon, builtin: p.builtin, count: p.defs.length, target: p.target, mix: [n("m"), n("f"), n("n")], computed, ready };
}

function detail(p: Preset): PresetDetail {
  return {
    ...view(p),
    voices: p.defs.map((d, index) => ({ ...d, index, hash: hash(d), langs: [...library].filter((k) => k.startsWith(`${hash(d)}:`)).map((k) => k.split(":").pop()!).sort() })),
  };
}

const engine: EngineStatus = {
  jobs: [
    {
      id: "job-demo1",
      preset_id: "cinematic",
      preset_name: "Cinematic",
      languages: ["de", "en"],
      options: { speech_check: true, takes: 2, backup: true },
      total: 60,
      done: 60,
      failed: 0,
      state: "done",
      phase: "",
      current: null,
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
  const p = all.find((x) => x.id === job.preset_id);
  const todo = p?.defs.flatMap((d) => job.languages.map((l) => [d, l] as const)).filter(([d, l]) => !library.has(`${hash(d)}:${l}`)) ?? [];
  job.state = "running";
  const next = todo[0];
  if (!next || job.done >= job.total) {
    Object.assign(job, { state: "done", phase: "", current: null, finished: Math.floor(Date.now() / 1000), eta_secs: null });
  } else {
    library.add(`${hash(next[0])}:${next[1]}`);
    job.done++;
    job.phase = ["design", "check", "save", "prime"][job.done % 4]!;
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
      const job: JobView = {
        id: `job-${Date.now()}`,
        preset_id: p.id,
        preset_name: p.name,
        languages: r.languages,
        options: r.options,
        total,
        done: 0,
        failed: 0,
        state: "queued",
        phase: "",
        current: null,
        created: Math.floor(Date.now() / 1000),
        finished: null,
        secs_per_voice: 10.8,
        eta_secs: Math.round(total * 10.8),
        error: null,
      };
      engine.jobs.push(job);
      if (!timer) timer = window.setInterval(() => tick(emit), 700);
      return { value: structuredClone(job) };
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
