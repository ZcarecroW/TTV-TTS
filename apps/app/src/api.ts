// Bridge to the Rust side. Outside Tauri (plain browser) a demo backend
// answers, for UI development, tests and the docs screenshots.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export interface VcpState {
  url: string;
  running: boolean;
  reachable: boolean;
  key_ok: boolean | null;
  version: string | null;
  error: string | null;
  model_loaded: boolean;
  model_kind: string | null;
  model_name: string | null;
  active_model: string | null;
  queue_size: number;
  generating: boolean;
  hardware: string | null;
  starting: boolean;
  exe: string | null;
  speed: number | null;
  usual_speed: number | null;
}

export interface ProxyInfo {
  id: string;
  name: string;
  version: string;
  port: number;
  fingerprint: string;
  pairing_open: boolean;
  os: string;
  vcp_reachable: boolean;
  vcp_version: string | null;
}

export interface ProxyStatus {
  info: ProxyInfo;
  vcp: VcpState;
  addresses: { interface: string; ip: string; prefix: number; mac: string }[];
  devices: { id: string; name: string; last_seen: number | null }[];
  connections: number;
  uptime_secs: number;
  update: ProxyUpdateState | null;
  /** set when this app is a friend's guest */
  guest: GuestInfo | null;
}

/** A friend who uses a proxy over the internet. */
export interface GuestInfo {
  id: string;
  name: string;
  chars_today: number;
  chars_per_day: number;
  joined: number;
  last_seen: number | null;
  last_address: string | null;
  total_chars: number;
}

export interface InviteView {
  id: string;
  name: string;
  created: number;
  expires: number;
  chars_per_day: number;
}

/** Internet access of the paired proxy. */
export interface InternetStatus {
  enabled: boolean;
  port: number;
  listening: boolean;
  error: string | null;
  domain: string;
  duckdns_token: boolean;
  duckdns_at: number | null;
  duckdns_result: string | null;
  public_ip: string | null;
  test_at: number | null;
  reachable: boolean | null;
  address: string;
  invites: InviteView[];
  guests: GuestInfo[];
  connections: number;
}

export const friends = {
  status: () => invoke<InternetStatus>("internet_status"),
  set: (patch: { enabled?: boolean; port?: number; domain?: string; token?: string }) => invoke<InternetStatus>("internet_set", { patch }),
  invite: (name: string, days: number, charsPerDay: number, address: string | null) => invoke<{ invite: InviteView; code: string }>("internet_invite", { name, days, charsPerDay, address }),
  withdraw: (id: string) => invoke<unknown>("internet_withdraw", { id }),
  revoke: (id: string) => invoke<unknown>("internet_revoke", { id }),
  test: () => invoke<{ reachable: boolean }>("internet_test"),
  read: (code: string) => invoke<{ host: string; address: string }>("read_invite", { code }),
  join: (code: string) => invoke<AppView>("join_invite", { code }),
};

export interface ProxyUpdateState {
  available: string | null;
  phase: "idle" | "checking" | "downloading" | "installing" | "error";
  progress: number | null;
  error: string | null;
  channel: string;
}

export interface HostStatus {
  kind: "none" | "proxy" | "direct";
  connected: boolean;
  error: string | null;
  proxy: ProxyStatus | null;
  vcp: VcpState | null;
}

export type HostConfig =
  | { kind: "none" }
  | { kind: "proxy"; id: string; name: string; address: string; fingerprint: string; mac: string | null; guest?: boolean }
  | { kind: "direct"; url: string };

export type GenerationMode = "auto" | "sentences" | "streaming";

export interface Config {
  device_id: string;
  host: HostConfig;
  generation: GenerationMode;
  audio: { device: string | null; volume: number; gap_ms: number };
  language: string;
  theme: string;
  update_channel: string;
  setup_done: boolean;
  translate_email: string;
}

export interface OutputDevice {
  id: string;
  name: string;
  default: boolean;
}

export interface AppView {
  config: Config;
  host: HostStatus;
  devices: OutputDevice[];
  version: string;
  data_dir: string;
  deepl_key: boolean;
}

export interface Found {
  address: string;
  info: ProxyInfo;
}

export interface Probe {
  address: string;
  info: ProxyInfo;
  fingerprint: string;
  fingerprint_short: string;
}

export interface Voice {
  id: string;
  name: string;
  transcript: string;
  voice_type: string;
  duration_seconds: number;
  has_voice_clone_prompt: boolean;
  created_with_model: string | null;
}

export interface Utterance {
  text: string;
  voice: string;
  language: string;
  emotion?: string | null;
  intensity?: string | null;
  gain?: number;
  /** Voice effect (one of EFFECTS). */
  fx?: string | null;
}

export interface SayReport {
  pieces: number;
  first_audio_ms: number;
  total_ms: number;
  audio_secs: number;
  rtf: number;
}

export interface GpuStats {
  name: string;
  usage_percent: number | null;
  vram_used: number | null;
  vram_total: number | null;
  temperature_c: number | null;
  power_w: number | null;
  processes: { pid: number; name: string; vram: number | null; usage: number | null }[];
}

export interface SystemStats {
  cpu_percent: number;
  mem_used: number;
  mem_total: number;
  gpus: GpuStats[];
}

export const isTauri = "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return tauriInvoke<T>(cmd, args);
  const { demoInvoke } = await import("./demo");
  return demoInvoke<T>(cmd, args);
}

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (isTauri) return tauriListen<T>(event, (e) => cb(e.payload));
  const { demoListen } = await import("./demo");
  return demoListen(event, cb as (p: unknown) => void);
}

export const api = {
  state: () => invoke<AppView>("get_state"),
  updateSettings: (patch: Partial<Pick<Config, "language" | "theme" | "update_channel" | "generation" | "setup_done" | "translate_email">>) => invoke<AppView>("update_settings", { patch }),
  discover: () => invoke<Found[]>("discover_proxies"),
  probe: (address: string) => invoke<Probe>("probe_proxy", { address }),
  pair: (address: string, fingerprint: string, code: string) => invoke<AppView>("pair_proxy", { address, fingerprint, code }),
  useDirect: (url: string, key: string | null) => invoke<AppView>("use_direct", { url, key }),
  forgetHost: () => invoke<AppView>("forget_host"),
  watchHostSystem: (on: boolean) => invoke<void>("watch_host_system", { on }),
  proxyVcpAction: (action: "start" | "stop" | "restart") => invoke<string>("proxy_vcp_action", { action }),
  wakeHost: () => invoke<void>("wake_host"),
  proxyUpdate: (action: "check" | "install") => invoke<ProxyUpdateState | null>("proxy_update", { action }),
  setAudio: (patch: { device?: string | null; volume?: number; gap_ms?: number }) => invoke<AppView>("set_audio", { patch }),
  voices: () => invoke<Voice[]>("list_voices"),
  speakTest: (utterance: Utterance) => invoke<SayReport>("speak_test", { utterance }),
  tryTranslate: (text: string, target: string, provider: string) => invoke<{ text: string; ms: number }>("try_translate", { text, from: null, target, provider }),
  setDeeplKey: (key: string | null) => invoke<AppView>("set_deepl_key", { key }),
  skip: () => invoke<void>("skip"),
};

export function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** VCP's language names, in the order shown in pickers. */
export const LANGUAGES = [
  ["German", "de"],
  ["English", "en"],
  ["Russian", "ru"],
  ["French", "fr"],
  ["Spanish", "es"],
  ["Italian", "it"],
  ["Portuguese", "pt"],
  ["Japanese", "ja"],
  ["Chinese", "zh"],
  ["Korean", "ko"],
] as const;

// ── chat reader ─────────────────────────────────────────────────────────

export type Role = "broadcaster" | "moderator" | "vip" | "subscriber" | "everyone";

export interface Profile {
  id: string;
  name: string;
  channels: string[];
  reading: {
    mode: "all" | "command" | "privileged";
    command: string;
    allowed_roles: Role[];
    reward_id: string;
    ignore_broadcaster: boolean;
    ignore_users: string[];
    ignore_prefixes: string[];
    read_sub_messages: boolean;
    max_chars: number;
    max_seconds: number;
    user_cooldown_secs: number;
    max_queue: number;
  };
  names: { announce: "on_change" | "always" | "never"; reset_secs: number; cleanup: boolean; aliases: Record<string, string> };
  language: { default: string; unsupported: string; remember_minutes: number };
  emotes: { mode: "once" | "remove" | "keep"; max_per_message: number; split_words: boolean; skip_emote_only: boolean; bttv: boolean; ffz: boolean; seventv: boolean; aliases: Record<string, string> };
  emojis: { mode: "describe" | "remove" | "keep"; max_per_message: number };
  text: {
    links: "replace" | "remove" | "skip";
    mentions: "name" | "remove";
    collapse_repeats: boolean;
    fix_shouting: boolean;
    replacements: Record<string, Record<string, string>>;
    blocked_words: string[];
    blocked_mode: "skip" | "beep";
  };
  mod_commands: { enabled: boolean; roles: Role[]; skip: string; clear: string; pause: string; resume: string; volume: string; mute: string; unmute: string; skip_on_timeout: boolean };
  voices: {
    mode: "preset" | "per_user_language" | "per_user" | "per_language" | "single";
    preset: string;
    preset_voices: number;
    prefix: string;
    fallback_language: string;
    exclude: string[];
    default_voice: string | null;
    per_language: Record<string, string>;
    users: Record<string, string>;
    emotions: boolean;
    emotion_strength: "mild" | "normal" | "strong";
    effects: boolean;
  };
  translate: { enabled: boolean; into: string; mine: string[]; read: "translation" | "both" | "show"; provider: "mymemory" | "deepl" | "vcp" };
  muted: string[];
}

export type ItemState = "queued" | "speaking" | "spoken" | "skipped" | "removed" | "failed" | "filtered";

export interface ItemView {
  id: number;
  channel: string;
  login: string;
  display: string;
  color: string | null;
  original: string;
  speech: string;
  lang: string;
  voice: string | null;
  state: ItemState;
  reason: string | null;
  at: number;
  first_audio_ms: number | null;
  emotion: string | null;
  intensity: string | null;
  translation: string | null;
}

export interface ReaderStatus {
  connected: boolean;
  error: string | null;
  channels: { name: string; joined: boolean; room_id: string | null }[];
  paused: boolean;
  queue: ItemView[];
  current: ItemView | null;
}

export interface ReaderView {
  store: { profiles: Profile[]; active: string[] };
  status: ReaderStatus;
  history: ItemView[];
}

export const reader = {
  get: () => invoke<ReaderView>("get_reader"),
  saveProfile: (profile: Profile) => invoke<ReaderView>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<ReaderView>("delete_profile", { id }),
  setActive: (ids: string[]) => invoke<ReaderView>("set_active_profiles", { ids }),
  skip: () => invoke<void>("reader_skip"),
  clear: () => invoke<void>("reader_clear"),
  pause: (on: boolean) => invoke<void>("reader_pause", { on }),
  test: (profile: string, user: string, text: string) => invoke<void>("test_message", { profile, user, text }),
  mute: (profile: string, login: string, on: boolean) => invoke<ReaderView>("mute_viewer", { profile, login, on }),
};

export interface UpdateInfo {
  available: string | null;
  phase: "idle" | "checking" | "downloading" | "error";
  error: string | null;
}

export const updates = {
  info: () => invoke<UpdateInfo>("update_info"),
  check: () => invoke<UpdateInfo>("check_update"),
  install: () => invoke<void>("install_update"),
};

// ── presets, computing, library ─────────────────────────────────────────

export type Gender = "m" | "f" | "n";

export interface VoiceDef {
  name: string;
  gender: Gender;
  age: number;
  description: string;
  fx: string | null;
}

export interface Preset {
  id: string;
  name: string;
  description: string;
  icon: string;
  builtin: boolean;
  target: number;
  defs: VoiceDef[];
}

export interface PresetView {
  id: string;
  name: string;
  description: string;
  icon: string;
  builtin: boolean;
  count: number;
  target: number;
  /** men, women, neutral among the first 30 */
  mix: [number, number, number];
  /** language -> computed voices */
  computed: Record<string, number>;
  /** voices the quality check lists (may sound odd) */
  odd: number;
  /** definitions computed in at least one language */
  ready: number;
}

export interface VoiceView extends VoiceDef {
  index: number;
  hash: string;
  langs: string[];
  /** languages whose voice may sound odd (quality check) */
  odd: string[];
}

export interface PresetDetail extends PresetView {
  voices: VoiceView[];
}

export interface Issue {
  line: number;
  level: "error" | "warning";
  message: string;
}

export interface Parsed {
  name: string;
  description: string;
  icon: string;
  defs: VoiceDef[];
  issues: Issue[];
}

export interface JobOptions {
  speech_check: boolean;
  takes: number;
  backup: boolean;
  /** quality check: list voices that may sound odd */
  quality?: boolean;
  /** design odd voices again automatically, keep the better one */
  improve?: boolean;
}

export type JobKind = "compute" | "improve" | "review" | "import";

/** One thing that may sound odd about a voice. */
export interface QualityIssue {
  code: string;
  value: number;
  weight: number;
  extra: string | null;
}

export interface Quality {
  /** 1 = nothing odd; below 0.75 the voice is listed */
  score: number;
  issues: QualityIssue[];
  f0: number | null;
  checked: number;
  new_text: boolean;
  kept: boolean;
}

export interface ReviewItem {
  key: string;
  preset_id: string;
  preset_name: string;
  index: number;
  name: string;
  gender: string;
  age: number;
  description: string;
  lang: string;
  quality: Quality;
}

export interface ReviewList {
  items: ReviewItem[];
  checked: number;
  unchecked: number;
  kept: number;
}

export interface PackPresetInfo {
  id: string;
  name: string;
  builtin: boolean;
  voices: number;
  /** "known" (the app has it), "new" (will be added), "copy" (added as a copy) */
  status: "known" | "new" | "copy";
}

/** What a voice pack holds. */
export interface PackInfo {
  path: string;
  title: string;
  created: number;
  app: string;
  bytes: number;
  voices: number;
  /** voices this library doesn't have yet */
  new_voices: number;
  languages: string[];
  presets: PackPresetInfo[];
}

/** A ready-made pack: voices from–to of a built-in preset in one language. */
export interface CatalogPack {
  file: string;
  preset: string;
  lang: string;
  from: number;
  to: number;
  voices: number;
  bytes: number;
  sha256: string;
}

export const packs = {
  catalog: () => invoke<{ version: number; updated: string; packs: CatalogPack[] }>("pack_catalog"),
  download: (files: string[]) => invoke<JobView[]>("download_packs", { files }),
  /** null when the user cancels */
  pick: () => invoke<string | null>("pick_pack"),
  info: (path: string) => invoke<PackInfo>("pack_info", { path }),
  import: (path: string) => invoke<{ job: JobView; presets: string[] }>("import_pack", { path }),
  /** null when the user cancels the save dialog */
  export: (presetId: string, languages: string[], count: number, keys: string[]) => invoke<{ path: string; voices: number; bytes: number } | null>("export_pack", { presetId, languages, count, keys }),
};

/** Shows a file in Explorer / Finder. */
export async function openerReveal(path: string) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}

/** Below this score the quality check lists a voice. */
export const FLAG = 0.75;

export type JobState = "queued" | "running" | "paused" | "waiting" | "done" | "cancelled" | "failed";

export interface JobView {
  id: string;
  preset_id: string;
  preset_name: string;
  languages: string[];
  options: JobOptions;
  kind: JobKind;
  total: number;
  done: number;
  failed: number;
  /** quality check: voices found that may sound odd */
  flagged: number;
  /** improve: voices replaced by a better design */
  improved: number;
  state: JobState;
  phase: string;
  current: string | null;
  /** Voices in the current batch; step/steps = progress inside its phase. */
  batch: number;
  step: number;
  steps: number;
  created: number;
  finished: number | null;
  secs_per_voice: number | null;
  eta_secs: number | null;
  error: string | null;
}

/** Result of checking the computed voices against VCP. */
export interface Verified {
  checked: number;
  removed: number;
  measured: number;
}

export interface EngineStatus {
  jobs: JobView[];
  secs_per_voice: number | null;
  library_size: number;
  library_rev: number;
}

export interface LibraryNotice {
  rev: number;
  size: number;
}

export interface ClassicFound {
  roles: number;
  voices: number;
  languages: string[];
}

export interface PromptRequest {
  theme: string;
  name: string;
  total: number;
  have: VoiceDef[];
  effects: boolean;
}

export const presets = {
  list: () => invoke<PresetView[]>("list_presets"),
  get: (id: string) => invoke<PresetDetail>("get_preset", { id }),
  save: (preset: Preset) => invoke<PresetView>("save_preset", { preset }),
  duplicate: (id: string) => invoke<PresetView>("duplicate_preset", { id }),
  remove: (id: string) => invoke<void>("delete_preset", { id }),
  parse: (text: string) => invoke<Parsed>("parse_preset", { text }),
  exportText: (id: string) => invoke<string>("export_preset", { id }),
  prompt: (request: PromptRequest) => invoke<string>("preset_prompt", { request }),
  engine: () => invoke<EngineStatus>("engine_status"),
  estimate: (voices: number, takes: number, check: boolean) => invoke<number | null>("estimate", { voices, takes, check }),
  benchmark: () => invoke<number>("benchmark"),
  compute: (request: { preset_id: string; count: number; languages: string[]; options: JobOptions }) => invoke<JobView>("compute", { request }),
  job: (id: string, action: "pause" | "resume" | "cancel" | "remove") => invoke<void>("job_control", { id, action }),
  deleteVoices: (keys: string[]) => invoke<number>("delete_voices", { keys }),
  redo: (presetId: string, index: number, lang: string, pick = false) => invoke<JobView>("redo_voice", { presetId, index, lang, pick }),
  review: () => invoke<ReviewList>("quality_list"),
  keep: (keys: string[], kept = true) => invoke<number>("keep_voices", { keys, kept }),
  improve: (keys: string[]) => invoke<JobView[]>("improve_voices", { keys }),
  checkQuality: (presetId: string | null, again = false) => invoke<JobView[]>("check_quality", { presetId, again }),
  playReference: (key: string) => invoke<number>("play_reference", { key }),
  verify: () => invoke<Verified>("verify_library"),
  listen: (presetId: string, index: number, lang: string, text: string) => invoke<SayReport>("listen", { presetId, index, lang, text }),
  detectClassic: () => invoke<ClassicFound>("detect_classic"),
  adoptClassic: () => invoke<{ roles: number; adopted: number }>("adopt_classic"),
};

/** Language codes voices can be designed for (with an accent and a check sentence). */
export const DESIGN_LANGS = ["en", "de", "ru", "fr", "es", "it", "pt", "ja", "zh", "ko"] as const;
/** Tiers of the built-in presets: their first 30, 60, 120 or all 240 voices. */
export const TIERS = [30, 60, 120, 240] as const;

// ── let me pick ─────────────────────────────────────────────────────────

export interface TakeInfo {
  n: number;
  duration: number;
  wer: number | null;
  score: number;
  recommended: boolean;
}

/** Takes of a voice designed again, waiting for a choice. */
export interface TakeChoice {
  key: string;
  name: string;
  lang: string;
  preset_id: string;
  index: number;
  created: number;
  takes: TakeInfo[];
}

export const takes = {
  list: () => invoke<TakeChoice[]>("list_takes"),
  play: (key: string, n: number) => invoke<number>("play_take", { key, n }),
  choose: (key: string, n: number) => invoke<void>("choose_take", { key, n }),
  discard: (key: string) => invoke<void>("discard_takes", { key }),
};

// ── viewers ─────────────────────────────────────────────────────────────

export interface ViewerView {
  user_id: string;
  login: string;
  display: string;
  color: string | null;
  last_seen: number;
  messages: number;
  role: number | null;
  voice: string | null;
  pinned: string | null;
  muted: boolean;
  /** Effect set for this viewer ("none" = off); null = the voice's own. */
  fx: string | null;
  /** The effect of the voice itself (from the preset). */
  voice_fx: string | null;
}

/** Built-in voice effects (same order as the backend). */
export const EFFECTS = ["radio", "telephone", "megaphone", "robot", "cave", "hall", "walkie", "underwater", "whisper"] as const;

export const viewers = {
  list: (profile: string, query: string) => invoke<{ viewers: ViewerView[]; total: number }>("list_viewers", { profile, query }),
  setRole: (profile: string, userId: string, role: number | null) => invoke<void>("set_viewer_role", { profile, userId, role }),
  setFx: (profile: string, userId: string, fx: string | null) => invoke<void>("set_viewer_fx", { profile, userId, fx }),
  forget: (profile: string, userId: string) => invoke<void>("forget_viewer", { profile, userId }),
};
