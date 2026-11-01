// Demo backend for the browser (UI development, tests, docs screenshots).
// `?setup` shows the first-start assistant, `?nohost` an app without voice host.
import type { AppView, Found, Probe, SystemStats, Voice } from "./api";
import { presetCommand } from "./demo-presets";
import { readerCommand, updateCommand } from "./demo-reader";

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
  active_model: "qwen3-tts",
  queue_size: 0,
  generating: false,
  hardware: "GPU: RTX 3070 (8GB)\nAccel: CUDA",
  starting: false,
  exe: "C:\\Games\\Voice Creator Pro\\VoiceCreatorPro.exe",
};

const state: AppView = {
  version: "2.0.0",
  data_dir: "C:\\Users\\you\\AppData\\Roaming\\io.github.zcarecrow.ttvtts",
  devices: [
    { id: "wasapi:speakers", name: "Speakers (Realtek(R) Audio)", default: true },
    { id: "wasapi:headset", name: "Headset (HyperX Cloud II)", default: false },
    { id: "wasapi:cable", name: "CABLE Input (VB-Audio Virtual Cable)", default: false },
  ],
  config: {
    device_id: "demo",
    host: params.has("nohost")
      ? { kind: "none" }
      : { kind: "proxy", id: proxyInfo.id, name: proxyInfo.name, address: "192.168.1.25:8110", fingerprint: proxyInfo.fingerprint, mac: "00:11:22:33:44:55" },
    generation: "auto",
    audio: { device: null, volume: 0.85, gap_ms: 250 },
    language: params.get("lang") ?? "system",
    theme: params.get("theme") ?? "system",
    update_channel: "stable",
    setup_done: !params.has("setup"),
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
        },
      },
};

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
