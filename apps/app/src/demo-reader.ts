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
  voices: { mode: "per_user_language", prefix: "TTS-{lang} ", fallback_language: "en", exclude: [], default_voice: null, per_language: {}, users: {}, emotions: false },
  muted: [],
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
    current: { ...item("Mikey_TV", "that boss fight was insane KEKW KEKW", "Mikey TV says: that boss fight was insane, kek", "en", "TTS-en 11 Gabriel (Hero)", "speaking", null, "#1E90FF"), first_audio_ms: 1240 },
  },
  history: [
    item("nightbot", "Follow on Twitter!", "", "", null, "filtered", "ignored user (bot)"),
    item("Anya_ru", "Привет всем! 🔥", "Anya ru пишет: Привет всем, огонь", "ru", "TTS-ru 16 Anastasia (Agentin)", "spoken", null, "#FF4500"),
    item("KlausDieter", "gg wp, bin gleich zurück", "Klaus Dieter sagt: gut gespielt well played, bin gleich zurück", "de", "TTS-de 13 Sebastian (Mentor)", "spoken", null, "#2E8B57"),
    item("spam_guy", "buy followers at cheap-views.com", "", "", null, "removed", "message deleted"),
    item("Lena_Plays", "hallo zusammen 👋", "Lena Plays sagt: hallo zusammen, winke winke", "de", "TTS-de 07 Elena (Abenteurerin)", "spoken", null, "#8A5CF6"),
    item("Mikey_TV", "!points", "", "", null, "filtered", "command / prefix"),
  ],
};

/** Handles reader commands; returns undefined for unknown ones. */
export function readerCommand(cmd: string, args: Record<string, unknown>, emit: (event: string, payload: unknown) => void): { value: unknown } | undefined {
  const v = (value: unknown) => ({ value });
  switch (cmd) {
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
      return v(undefined);
    case "test_message": {
      const user = String(args.user || "TestUser");
      emit("app://history", item(user, String(args.text), user + " sagt: " + String(args.text), "de", "TTS-de 01 Konrad (Trailer-Erzähler)", "spoken"));
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
