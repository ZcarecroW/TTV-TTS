// Demo backend: chat reader (profiles, queue, history). Used by demo.ts.
import type { ItemView, Profile, ReaderView } from "./api";

const params = new URLSearchParams(location.search);
const off = params.has("nohost");

export const baseProfile: Profile = {
  id: "demo1",
  name: "My channel",
  channels: ["zcarecrow"],
  reading: {
    mode: "all",
    command: "!tts",
    allowed_roles: ["broadcaster", "moderator", "vip", "subscriber"],
    reward_id: "",
    ignore_broadcaster: false,
    ignore_users: ["nightbot", "streamelements", "wizebot", "moobot"],
    ignore_prefixes: ["!"],
    read_sub_messages: true,
    max_chars: 400,
    max_seconds: 60,
    user_cooldown_secs: 0,
    max_queue: 15,
    approval: false,
    approval_minutes: 10,
  },
  names: { announce: "on_change", reset_secs: 90, cleanup: true, aliases: { xx_gamer_xx: "Gamer" } },
  language: { default: "de", unsupported: "auto", remember_minutes: 15 },
  emotes: { mode: "once", max_per_message: 3, split_words: true, skip_emote_only: false, bttv: true, ffz: true, seventv: true, aliases: { KEKW: "kek", OMEGALUL: "omega lul", Sadge: "sadge" } },
  emojis: { mode: "describe", max_per_message: 2 },
  text: {
    links: "replace",
    mentions: "name",
    collapse_repeats: true,
    fix_shouting: true,
    replacements: { all: { "w/": "with" }, de: { gg: "gut gespielt", brb: "bin gleich zurück" }, en: { gg: "good game", brb: "be right back" } },
    blocked_words: [],
    blocked_mode: "skip",
  },
  mod_commands: { enabled: true, roles: ["broadcaster", "moderator"], skip: "!skip", clear: "!ttsclear", pause: "!ttspause", resume: "!ttsresume", volume: "!ttsvol", mute: "!ttsmute", unmute: "!ttsunmute", skip_on_timeout: true },
  voices: { mode: "preset", preset: "cinematic", preset_voices: 30, prefix: "TTS-{lang} ", fallback_language: "en", exclude: [], default_voice: null, per_language: {}, users: {}, emotions: true, emotion_strength: "normal", effects: true, os_fallback: false, viewer_plugins: { share: 25, skip_voice_fx: true } },
  translate: { enabled: true, into: "", mine: ["en"], read: "both", provider: "mymemory" },
  events: { enabled: true, subs: true, gifts: true, raids: true, bits: true, follows: false, redemptions: true, min_bits: 100, min_raid: 1, voice: "", texts: {} },
  muted: [],
  output: { mode: "device", token: "Xk3vQ9pLm2RtW7aZ", online: false },
};

let nextId = 100;
function item(display: string, original: string, speech: string, lang: string, voice: string | null, state: ItemView["state"], reason: string | null = null, color: string | null = null): ItemView {
  const id = nextId++;
  return {
    id,
    channel: "zcarecrow",
    login: display.toLowerCase(),
    display,
    color,
    original,
    speech,
    lang,
    voice,
    state,
    reason,
    at: Date.now() - (120 - id) * 9000,
    first_audio_ms: state === "spoken" ? 1100 + (id % 7) * 90 : null,
    emotion: null,
    intensity: null,
    translation: null,
    event: null,
  };
}

const friend: Profile = { ...structuredClone(baseProfile), id: "demo2", name: "Friend's stream", channels: ["some_friend"], language: { default: "en", unsupported: "auto", remember_minutes: 15 } };

export const readerState: ReaderView = {
  store: { profiles: [baseProfile, friend], active: off ? [] : ["demo1"] },
  status: {
    connected: !off,
    error: null,
    channels: off ? [] : [{ name: "zcarecrow", joined: true, room_id: "123" }],
    paused: false,
    queue: [item("Lena_Plays", "wie heißt das spiel? 🤔", "Lena Plays sagt: wie heißt das spiel?, hmm", "de", "TTS-de 07 Elena (Abenteurerin)", "queued", null, "#8A5CF6")],
    current: { ...item("Mikey_TV", "that boss fight was insane KEKW KEKW", "Mikey TV says: that boss fight was insane, kek", "en", "TTS-en 11 Gabriel (Hero)", "speaking", null, "#1E90FF"), first_audio_ms: 1240, emotion: "joy", intensity: "normal" },
    pending: params.has("approval") ? [item("Grumpy_Gus", "this streamer is the worst lol", "Grumpy Gus says: this streamer is the worst lol", "en", null, "pending", null, "#B22222")] : [],
    replayable: [],
  },
  history: [
    { ...item("BigStreamer", "", "BigStreamer raidet mit 250 Zuschauern!", "de", "fem_ger_wow", "spoken", null, "#9146FF"), event: "raid" },
    { ...item("Pablo_Juega", "hola a todos, ¿qué juego es este?", "Pablo Juega dice: hola a todos, ¿qué juego es este?", "es", "TTS-es 04 Diego (Abenteurer)", "spoken", null, "#FF7F50"), translation: "Hallo zusammen, welches Spiel ist das?" },
    item("nightbot", "Follow on Twitter!", "", "", null, "filtered", "ignored user (bot)"),
    item("Anya_ru", "Привет всем! 🔥", "Anya ru пишет: Привет всем, огонь", "ru", "TTS-ru 16 Anastasia (Agentin)", "spoken", null, "#FF4500"),
    item("KlausDieter", "gg wp, bin gleich zurück", "Klaus Dieter sagt: gut gespielt well played, bin gleich zurück", "de", "TTS-de 13 Sebastian (Mentor)", "spoken", null, "#2E8B57"),
    item("spam_guy", "buy followers at cheap-views.com", "", "", null, "removed", "message deleted"),
    item("Lena_Plays", "hallo zusammen 👋", "Lena Plays sagt: hallo zusammen, winke winke", "de", "TTS-de 07 Elena (Abenteurerin)", "spoken", null, "#8A5CF6"),
    item("Mikey_TV", "!points", "", "", null, "filtered", "command / prefix"),
  ],
};

// spoken messages can be played again
readerState.status.replayable = readerState.history.filter((h) => h.state === "spoken").map((h) => h.id);

/** Handles reader commands; returns undefined for unknown ones. */
export function readerCommand(cmd: string, args: Record<string, unknown>, emit: (event: string, payload: unknown) => void): { value: unknown } | undefined {
  const v = (value: unknown) => ({ value });
  switch (cmd) {
    case "list_viewers": {
      const now = Math.floor(Date.now() / 1000);
      const all = [
        { user_id: "1", login: "mikey_tv", display: "Mikey_TV", color: "#1E90FF", last_seen: now - 20, messages: 214, role: 3, voice: "Imogen Vale", pinned: null, muted: false, fx: null, voice_fx: null, plugin: "chip", plugin_manual: false },
        { user_id: "2", login: "lenaplays", display: "LenaPlays", color: "#FF69B4", last_seen: now - 75, messages: 88, role: 7, voice: "Marcus Thorne", pinned: null, muted: false, fx: "radio", voice_fx: null, plugin: "demon", plugin_manual: true },
        { user_id: "3", login: "xx_gamer_xx", display: "xx_Gamer_xx", color: "#9ACD32", last_seen: now - 400, messages: 41, role: 12, voice: "Theo Pike", pinned: null, muted: true, fx: null, voice_fx: "robot" },
        { user_id: "4", login: "oma_hilde", display: "Oma_Hilde", color: null, last_seen: now - 3600, messages: 12, role: null, voice: null, pinned: "fem_ger_wow", muted: false, fx: null, voice_fx: null },
      ];
      const q = String(args.query ?? "").toLowerCase();
      const viewers = all.filter((v) => !q || v.login.includes(q));
      return v({ viewers, total: viewers.length });
    }
    case "set_viewer_role":
    case "set_viewer_fx":
    case "forget_viewer":
      return v(undefined);
    case "get_reader":
      return v(structuredClone(readerState));
    case "save_profile": {
      const p = args.profile as Profile;
      if (!p.id) p.id = "demo" + Date.now();
      const i = readerState.store.profiles.findIndex((x) => x.id === p.id);
      if (i >= 0) readerState.store.profiles[i] = p;
      else readerState.store.profiles.push(p);
      return v(structuredClone(readerState));
    }
    case "delete_profile":
      readerState.store.profiles = readerState.store.profiles.filter((x) => x.id !== args.id);
      return v(structuredClone(readerState));
    case "set_active_profiles":
      readerState.store.active = args.ids as string[];
      return v(structuredClone(readerState));
    case "reader_pause":
      readerState.status.paused = !!args.on;
      emit("app://reader", structuredClone(readerState.status));
      return v(undefined);
    case "reader_skip":
    case "reader_clear":
    case "reader_replay":
      return v(undefined);
    case "reader_approve":
    case "reader_reject": {
      const i = readerState.status.pending.findIndex((p) => args.id == null || p.id === args.id);
      if (i < 0) return v(false);
      const [it] = readerState.status.pending.splice(i, 1);
      if (cmd === "reader_approve") readerState.status.queue.push({ ...it!, state: "queued" });
      emit("app://reader", structuredClone(readerState.status));
      return v(true);
    }
    case "test_event": {
      const it = item("TestRaider", "", "TestRaider raidet mit 42 Zuschauern!", "de", "fem_ger_wow", "spoken");
      emit("app://history", { ...it, event: String(args.kind) });
      return v(undefined);
    }
    case "test_message": {
      const user = String(args.user || "TestUser");
      const text = String(args.text);
      const it = item(user, text, user + " sagt: " + text, "de", "TTS-de 01 Konrad (Trailer-Erzähler)", "spoken");
      if (/haha|KEKW|😂|lol/i.test(text)) Object.assign(it, { emotion: "joy", intensity: "mild" });
      else if (/rip|😭|nooo/i.test(text)) Object.assign(it, { emotion: "sad", intensity: "mild" });
      emit("app://history", it);
      return v(undefined);
    }
    case "mute_viewer":
      return v(structuredClone(readerState));
  }
  return undefined;
}

export function updateCommand(cmd: string): { value: unknown } | undefined {
  if (cmd === "update_info" || cmd === "check_update") return { value: { available: params.has("update") ? "2.0.1" : null, phase: "idle", error: null } };
  if (cmd === "install_update") return { value: undefined };
  return undefined;
}
