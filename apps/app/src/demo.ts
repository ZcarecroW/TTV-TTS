// Demo backend for the browser (UI development, tests, docs screenshots).
// `?setup` shows the first-start assistant, `?nohost` an app without voice host,
// `?guest` an app that uses a friend's voice host over the internet.
import { demoFriends, type InternetPatch } from "@ttv-tts/ui/friends";
import type { AppView, Found, Probe, Profile, SystemStats, Voice } from "./api";
import { presetCommand } from "./demo-presets";
import { readerCommand, readerState, updateCommand } from "./demo-reader";

const params = new URLSearchParams(location.search);
const listeners = new Map<string, Set<(p: unknown) => void>>();
const now = () => Math.floor(Date.now() / 1000);

const proxyInfo = {
  id: "k3f9a0c1d2e7",
  name: "VOICE-PC",
  version: "2.0.0",
  port: 8110,
  fingerprint: "3f9a0c1277de41b0c55e0a9d7b1f2e3c4d5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c",
  pairing_open: false,
  os: "windows",
  vcp_reachable: true,
  vcp_version: "2.4.12.0",
};
const vcp = {
  url: "http://127.0.0.1:8100",
  running: true,
  reachable: true,
  key_ok: true,
  version: "2.4.12.0",
  error: null,
  model_loaded: true,
  model_kind: "clone",
  model_name: "Higher Quality (1.7B)",
  speed: 0.52,
  usual_speed: 0.5,
  active_model: "qwen3-tts",
  queue_size: 0,
  generating: false,
  hardware: "GPU: RTX 3070 (8GB)\nAccel: CUDA",
  starting: false,
  exe: "C:\\Games\\Voice Creator Pro\\VoiceCreatorPro.exe",
};

const demoTwitch = { available: true, own_client_id: false, login: null as string | null, connected: false, subscribed: [] as string[], error: null as string | null, pending: null as { user_code: string; verification_uri: string; expires_at: number } | null };

const demoFound = [
  { format: "vst3" as const, path: "C:/Program Files/Common Files/VST3/Clean EQ.vst3", id: "1", name: "Clean EQ", vendor: "Example Audio" },
  { format: "clap" as const, path: "C:/Program Files/Common Files/CLAP/Room Verb.clap", id: "room.verb", name: "Room Verb", vendor: "Example Audio" },
  { format: "clap" as const, path: "C:/Plugins/Soft Comp.clap", id: "soft.comp", name: "Soft Comp", vendor: "Studio Nine" },
];
const demoPlugins = { slots: [{ id: "eq1", plugin: demoFound[0]!, enabled: true, state: null as string | null }], errors: {} as Record<string, string>, editors: true };

const state: AppView = {
  version: "2.0.0",
  data_dir: "C:\\Users\\you\\AppData\\Roaming\\io.github.zcarecrow.ttvtts",
  deepl_key: false,
  devices: [
    { id: "wasapi:speakers", name: "Speakers (Realtek(R) Audio)", default: true },
    { id: "wasapi:headset", name: "Headset (HyperX Cloud II)", default: false },
    { id: "wasapi:cable", name: "CABLE Input (VB-Audio Virtual Cable)", default: false },
  ],
  config: {
    device_id: "demo",
    host: params.has("nohost")
      ? { kind: "none" }
      : { kind: "proxy", id: proxyInfo.id, name: proxyInfo.name, address: params.has("guest") ? "alex-stream.duckdns.org:8111" : "192.168.1.25:8110", fingerprint: proxyInfo.fingerprint, mac: "00:11:22:33:44:55", guest: params.has("guest") },
    generation: "auto",
    audio: { device: null, volume: 0.85, gap_ms: 250 },
    language: params.get("lang") ?? "system",
    theme: params.get("theme") ?? "system",
    update_channel: "stable",
    setup_done: !params.has("setup"),
    translate_email: "",
    hotkeys: { skip: "Ctrl+Alt+KeyS" },
    remote: { enabled: false, port: 8130, lan: false, token: "demoToken123" },
    obs: { port: 8131, lan: false, public_host: "", https: true },
    effects_on_host: true,
  },
  host: params.has("nohost")
    ? { kind: "none", connected: false, error: null, proxy: null, vcp: null }
    : {
        kind: "proxy",
        connected: true,
        error: null,
        vcp,
        proxy: {
          info: proxyInfo,
          vcp,
          addresses: [{ interface: "Ethernet", ip: "192.168.1.25", prefix: 24, mac: "00:11:22:33:44:55" }],
          devices: [{ id: "demo", name: "GAMING-PC", last_seen: now() - 5 }],
          connections: 1,
          uptime_secs: 5 * 3600 + 720,
          update: { available: null, phase: "idle", progress: null, error: null, channel: "stable" },
          guest: params.has("guest") ? { id: "g1", name: "You", chars_today: 4200, chars_per_day: 30000, joined: now() - 86400, last_seen: now(), last_address: null, total_chars: 51200 } : null,
        },
      },
};

// Internet access of the demo proxy (friends).
const internet = demoFriends();

const voices: Voice[] = [
  ["TTS-de 01 Konrad (Trailer-Erzähler)", "design"],
  ["TTS-de 02 Marlene (Noir-Erzählerin)", "design"],
  ["TTS-de 04 Iris (Raumschiff-KI)", "design"],
  ["TTS-en 01 Grant (Trailer Narrator)", "design"],
  ["TTS-en 06 Elowen (Elf Queen)", "design"],
  ["fem_ger_wow", "clone"],
].map(([name, voice_type], i) => ({
  id: `voice_demo${i}`,
  name: name!,
  transcript: "",
  voice_type: voice_type!,
  duration_seconds: 7.5,
  has_voice_clone_prompt: true,
  created_with_model: "qwen3-tts",
}));

let sysTimer = 0;
function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((cb) => cb(payload));
}
function system(): SystemStats {
  const t = Date.now() / 1000;
  const wave = (p: number, a: number, b: number) => a + (b - a) * (0.5 + 0.5 * Math.sin(t / p));
  return {
    cpu_percent: wave(7, 4, 18),
    mem_used: 14.2 * 2 ** 30,
    mem_total: 32 * 2 ** 30,
    gpus: [
      {
        name: "NVIDIA GeForce RTX 3070",
        usage_percent: wave(5, 2, 61),
        vram_used: 6.9 * 2 ** 30,
        vram_total: 8 * 2 ** 30,
        temperature_c: Math.round(wave(9, 47, 63)),
        power_w: wave(5, 40, 170),
        processes: [
          { pid: 14212, name: "VoiceCreatorPro.exe", vram: 5.8 * 2 ** 30, usage: wave(5, 1, 58) },
          { pid: 8120, name: "dwm.exe", vram: 0.3 * 2 ** 30, usage: 1 },
        ],
      },
    ],
  };
}

export async function demoInvoke<T>(cmd: string, input: Record<string, unknown> = {}): Promise<T> {
  // Like Tauri: arguments arrive as JSON (no Vue proxies).
  const args = JSON.parse(JSON.stringify(input)) as Record<string, unknown>;
  await new Promise((r) => setTimeout(r, 80));
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case "get_state":
      return r(structuredClone(state));
    case "update_settings":
      Object.assign(state.config, args.patch);
      return r(structuredClone(state));
    case "discover_proxies":
      await new Promise((res) => setTimeout(res, 900));
      return r([{ address: "192.168.1.25:8110", info: proxyInfo }] satisfies Found[]);
    case "probe_proxy":
      return r({ address: String(args.address), info: proxyInfo, fingerprint: proxyInfo.fingerprint, fingerprint_short: "3F9A-0C12-77DE-41B0" } satisfies Probe);
    case "internet_status":
      return r(await internet.status());
    case "internet_set":
      return r(await internet.set(args.patch as InternetPatch));
    case "internet_test":
      return r(await internet.test());
    case "internet_invite":
      return r(await internet.invite(String(args.name), Number(args.days), Number(args.charsPerDay), (args.address as string) ?? null));
    case "internet_withdraw":
      return r(await internet.withdraw(String(args.id)));
    case "internet_revoke":
      return r(await internet.revoke(String(args.id)));
    case "read_invite":
      if (!String(args.code).trim().startsWith("ttv1:") || String(args.code).trim().length < 20) throw "This isn't a TTV-TTS invite code (it starts with ttv1:).";
      return r({ host: "ALEX-PC", address: "alex-stream.duckdns.org:8111" });
    case "join_invite":
      return r(structuredClone(state));
    case "pair_proxy":
      if (args.code !== "482913") throw "Wrong code - check the code shown in TTV-TTS Proxy.";
      return r(structuredClone(state));
    case "use_direct":
    case "forget_host":
    case "set_audio":
      if (cmd === "set_audio") Object.assign(state.config.audio, args.patch);
      return r(structuredClone(state));
    case "watch_host_system":
      clearInterval(sysTimer);
      if (args.on) {
        emit("app://host-system", system());
        sysTimer = window.setInterval(() => emit("app://host-system", system()), 2000);
      }
      return r(undefined);
    case "proxy_vcp_action":
      return r("Voice Creator Pro is up after 31 s.");
    case "wake_host":
      return r(undefined);
    case "proxy_update":
      return r(args.action === "check" ? { available: null, phase: "idle", progress: null, error: null, channel: "stable" } : null);
    case "list_voices":
      return r(voices);
    case "try_translate": {
      await new Promise((res) => setTimeout(res, 500));
      const known: Record<string, string> = { "hola a todos": "Hallo zusammen", "привет": "Hallo" };
      const text = String(args.text);
      const hit = Object.entries(known).find(([k]) => text.toLowerCase().includes(k));
      return r({ text: hit ? hit[1] + "!" : "(Übersetzung) " + text, ms: 480 });
    }
    case "twitch_status":
      return r(structuredClone(demoTwitch));
    case "twitch_login":
      demoTwitch.pending = { user_code: "QBQLNSHC", verification_uri: "https://www.twitch.tv/activate?device-code=QBQLNSHC", expires_at: now() + 1800 };
      setTimeout(() => {
        if (!demoTwitch.pending) return;
        Object.assign(demoTwitch, { pending: null, login: "zcarecrow", connected: true, subscribed: ["channel.follow", "channel.channel_points_custom_reward_redemption.add"] });
        emit("app://twitch", structuredClone(demoTwitch));
      }, 1500);
      return r(structuredClone(demoTwitch));
    case "twitch_cancel_login":
      demoTwitch.pending = null;
      return r(structuredClone(demoTwitch));
    case "twitch_logout":
      Object.assign(demoTwitch, { login: null, connected: false, subscribed: [] });
      return r(structuredClone(demoTwitch));
    case "set_twitch_client_id":
      demoTwitch.own_client_id = !!String(args.id).trim();
      return r(structuredClone(demoTwitch));
    case "plugins_list":
      return r(structuredClone(demoPlugins));
    case "plugins_scan":
      await new Promise((res) => setTimeout(res, 600));
      return r(demoFound);
    case "plugins_pick":
      return r([demoFound[2]]);
    case "plugins_add":
      demoPlugins.slots.push({ id: "p" + Date.now(), plugin: args.plugin as never, enabled: true, state: null });
      return r(structuredClone(demoPlugins));
    case "plugins_remove":
      demoPlugins.slots = demoPlugins.slots.filter((s) => s.id !== args.id);
      return r(structuredClone(demoPlugins));
    case "plugins_toggle": {
      const s = demoPlugins.slots.find((x) => x.id === args.id);
      if (s) s.enabled = !!args.enabled;
      return r(structuredClone(demoPlugins));
    }
    case "plugins_move": {
      const i = demoPlugins.slots.findIndex((x) => x.id === args.id);
      const j = Math.max(0, Math.min(demoPlugins.slots.length - 1, i + Number(args.delta)));
      [demoPlugins.slots[i], demoPlugins.slots[j]] = [demoPlugins.slots[j]!, demoPlugins.slots[i]!];
      return r(structuredClone(demoPlugins));
    }
    case "plugins_edit":
      return r(undefined);
    case "generation_info":
      return r({ host: "proxy:demo", model: "Higher Quality (1.7B)", at: Math.floor(Date.now() / 1000) - 3600, sentences_first_ms: 2600, sentences_rtf: 0.51, streaming_first_ms: 2800, streaming_rtf: 3.6, pick: "sentences" });
    case "measure_generation":
      await new Promise((res) => setTimeout(res, 1200));
      return r({ host: "proxy:demo", model: "Higher Quality (1.7B)", at: Math.floor(Date.now() / 1000), sentences_first_ms: 2400, sentences_rtf: 0.5, streaming_first_ms: 2900, streaming_rtf: 3.9, pick: "sentences" });
    case "set_remote":
      Object.assign(state.config.remote, { enabled: !!args.enabled, port: Number(args.port), lan: !!args.lan });
      if (args.newToken) state.config.remote.token = "newToken" + Date.now().toString(36);
      return r(structuredClone(state));
    case "obs_status": {
      const profiles = (readerState.store.profiles as Profile[]).filter((p) => p.output.mode === "obs");
      const c = state.config.obs;
      const lan = c.lan || profiles.some((p) => p.output.online);
      const duck = /\.duckdns\.org$/i.test(c.public_host);
      return r({
        listening: profiles.length ? `${lan ? "0.0.0.0" : "127.0.0.1"}:${c.port}` : null,
        error: null,
        links: profiles.map((p) => ({
          profile: p.id,
          local: `http://127.0.0.1:${c.port}/o/${p.output.token}`,
          lan: lan ? [`http://192.168.178.40:${c.port}/o/${p.output.token}`] : [],
          online: p.output.online && c.public_host ? `${duck && c.https ? "https" : "http"}://${c.public_host}:${c.port}/o/${p.output.token}` : null,
          online_secure: duck && c.https,
          securing: false,
          listeners: params.has("obs") ? 1 : 0,
        })),
        duckdns: duck,
        duckdns_token: duck,
        duckdns_update: duck ? { at: Math.floor(Date.now() / 1000) - 120, error: null, ip: "203.0.113.42" } : null,
        https: duck
          ? { domain: c.public_host, valid_from: Math.floor(Date.now() / 1000) - 86400, valid_until: Math.floor(Date.now() / 1000) + 89 * 86400, renew_at: Math.floor(Date.now() / 1000) + 59 * 86400, step: null, error: null, error_at: null, next_try: null, staging: false }
          : { domain: null, valid_from: null, valid_until: null, renew_at: null, step: null, error: null, error_at: null, next_try: null, staging: false },
      });
    }
    case "set_obs":
      Object.assign(state.config.obs, { lan: !!args.lan, port: Number(args.port), public_host: String(args.publicHost ?? ""), https: args.https !== false });
      return demoInvoke("obs_status");
    case "obs_new_link": {
      const p = (readerState.store.profiles as Profile[]).find((x) => x.id === args.profile);
      if (p) p.output.token = Math.random().toString(36).slice(2, 10) + Math.random().toString(36).slice(2, 10);
      return r(structuredClone(readerState));
    }
    case "hosted_list": {
      const profiles = (readerState.store.profiles as Profile[]).filter((p) => p.output.mode === "host_obs");
      return r(
        profiles.map((p) => ({
          id: p.id,
          name: p.name,
          channels: p.channels,
          online: p.output.online,
          listeners: params.has("obs") ? 1 : 0,
          reading: params.has("obs"),
          status: { ...readerState.status, current: params.has("obs") ? readerState.history[0] : null, queue: [] },
          lan: [`http://192.168.178.25:8112/o/${p.output.token}`],
          internet: p.output.online ? `https://mystream.duckdns.org:8112/o/${p.output.token}` : null,
          internet_secure: p.output.online,
          securing: false,
        })),
      );
    }
    case "hosted_history":
      return r(readerState.history);
    case "hosted_control":
    case "hosted_viewer":
      return r(null);
    case "hosted_test":
      if (!params.has("obs")) throw "Nobody listens: open the link in OBS first.";
      return r(null);
    case "hosted_viewers":
      return r(readerCommand("list_viewers", args, () => {})?.value);
    case "remote_status":
      return r(state.config.remote.enabled ? { listening: `127.0.0.1:${state.config.remote.port}`, error: null } : { listening: null, error: null });
    case "set_hotkey": {
      const hk = state.config.hotkeys;
      if (args.accel && Object.entries(hk).some(([k, v]) => k !== args.action && v === args.accel)) throw `This combination is already used for "${args.action}".`;
      if (args.accel) hk[String(args.action)] = String(args.accel);
      else delete hk[String(args.action)];
      return r(structuredClone(state));
    }
    case "set_deepl_key":
      state.deepl_key = !!args.key;
      return r(structuredClone(state));
    case "speak_test":
      await new Promise((res) => setTimeout(res, 1200));
      return r({ pieces: 2, first_audio_ms: 1180, total_ms: 6400, audio_secs: 5.2, rtf: 0.51 });
    case "skip":
      return r(undefined);
  }
  const fromReader = readerCommand(cmd, args, emit) ?? updateCommand(cmd) ?? presetCommand(cmd, args, emit);
  if (fromReader) return r(fromReader.value);
  throw new Error(`demo: unknown command ${cmd}`);
}

export function demoListen(event: string, cb: (p: unknown) => void) {
  let set = listeners.get(event);
  if (!set) listeners.set(event, (set = new Set()));
  set.add(cb);
  return () => set.delete(cb);
}
