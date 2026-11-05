// Demo backend for the browser (UI development, tests, docs screenshots).
// `?setup` shows the first-start assistant, `?problems` a broken setup.
// All keys here are made up.
import { demoFriends, type InternetPatch } from "@ttv-tts/ui/friends";
import type { Config, FirewallStatus, LogLine, Overview, SystemStats } from "./api";

const params = new URLSearchParams(location.search);
const problems = params.has("problems");
const listeners = new Map<string, Set<(p: unknown) => void>>();
const now = () => Math.floor(Date.now() / 1000);
const FAKE_VCP_KEY = "demo-vcp-key-0000-1111-2222-3333";

const config: Config = {
  id: "k3f9a0c1d2e7",
  name: "",
  port: 8110,
  discovery: true,
  vcp_url: "http://127.0.0.1:8100",
  key_mode: "vcp",
  vcp_exe: "C:\\Games\\Voice Creator Pro\\Voice Creator Pro.exe",
  start_vcp: false,
  keep_vcp_running: false,
  restart_on_hang: false,
  restart_when_slow: false,
  hang_restart_secs: 180,
  http_port: 0,
  obs_port: 8112,
  autostart: false,
  keep_awake: true,
  update_channel: "stable",
  auto_update: true,
  language: params.get("lang") ?? "system",
  theme: params.get("theme") ?? "system",
  setup_done: !params.has("setup"),
};

const overview: Overview = {
  config,
  has_vcp_key: !problems,
  fingerprint_short: "3F9A-0C12-77DE-41B0",
  data_dir: "C:\\Users\\you\\AppData\\Roaming\\io.github.zcarecrow.ttvtts.proxy",
  log_path: "C:\\Users\\you\\AppData\\Roaming\\io.github.zcarecrow.ttvtts.proxy\\logs\\proxy.log",
  listen_error: null,
  status: {
    info: {
      id: config.id,
      name: "VOICE-PC",
      version: "2.0.0",
      api: 1,
      port: 8110,
      fingerprint: "3f9a0c1277de41b0c55e0a9d7b1f2e3c4d5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c",
      pairing_open: false,
      os: "windows",
      vcp_reachable: !problems,
      vcp_version: problems ? null : "2.4.12.0",
    },
    vcp: {
      url: config.vcp_url,
      running: !problems,
      pid: problems ? null : 14212,
      exe: config.vcp_exe,
      reachable: !problems,
      key_ok: problems ? null : true,
      key_required: problems ? null : true,
      version: problems ? null : "2.4.12.0",
      error: problems ? "VCP is not reachable: connection refused" : null,
      model_loaded: !problems,
      model_kind: "base",
      model_name: "Higher Quality (1.7B)",
      speed: 0.55,
      usual_speed: 0.5,
      active_model: "qwen3-tts",
      queue_size: 0,
      generating: false,
      hardware: "GPU: RTX 3070 (8GB)\nAccel: CUDA",
      starting: false,
    },
    addresses: [{ interface: "Ethernet", ip: "192.168.1.25", prefix: 24, mac: "00:11:22:33:44:55" }],
    devices: problems
      ? []
      : [{ id: "gaming-pc", name: "Gaming PC", paired_at: now() - 86400 * 3, last_seen: now() - 40, last_address: "192.168.1.43" }],
    connections: problems ? 0 : 2,
    uptime_secs: 3 * 3600 + 1240,
    awake: "computing",
    update: { available: null, phase: "idle", progress: null, error: null, channel: "stable" },
  },
};

const firewall: FirewallStatus = {
  managed: true,
  rule: problems
    ? null
    : { enabled: true, action: "Allow", profile: "Private, Domain", program: "C:\\Users\\you\\AppData\\Local\\TTV-TTS Proxy\\ttv-tts-proxy.exe" },
  rule_ok: !problems,
  allows: problems ? 0 : 1,
  blocks: problems ? 1 : 0,
  profiles: [{ alias: "Ethernet", category: problems ? "Public" : "Private" }],
  public_network_blocked: false,
  wfc: problems,
  third_party: [],
  firewall_on: ["Domain", "Private", "Public"],
  program: "C:\\Users\\you\\AppData\\Local\\TTV-TTS Proxy\\ttv-tts-proxy.exe",
  error: null,
};

const logs: LogLine[] = [
  ["info", "TTV-TTS Proxy 2.0.0 starting"],
  ["info", "listening on port 8110 (HTTPS)"],
  ["info", "found Voice Creator Pro at C:\\Games\\Voice Creator Pro\\Voice Creator Pro.exe"],
  ["info", 'paired with "Gaming PC" (192.168.1.43)'],
  ["warn", "192.168.1.50 used a wrong key for /api/v1/voices"],
].map(([level, message], i) => ({ t: Date.now() - (5 - i) * 61_000, level: level!, message: message! }));

let accessKey = "demo-access-key-aaaa-bbbb-cccc-dd";
let systemTimer = 0;

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((cb) => cb(payload));
}

function system(): SystemStats {
  const t = Date.now() / 1000;
  const wave = (p: number, a: number, b: number) => a + (b - a) * (0.5 + 0.5 * Math.sin(t / p));
  return {
    cpu_percent: wave(7, 6, 22),
    mem_used: 11.4 * 2 ** 30,
    mem_total: 31.9 * 2 ** 30,
    gpus: [
      {
        name: "NVIDIA GeForce RTX 3070",
        vendor: "NVIDIA",
        usage_percent: wave(5, 3, 64),
        vram_used: 4.6 * 2 ** 30,
        vram_total: 8 * 2 ** 30,
        temperature_c: Math.round(wave(9, 46, 61)),
        power_w: wave(5, 38, 168),
        processes: [
          { pid: 14212, name: "VoiceCreatorPro.exe", vram: 3.9 * 2 ** 30, usage: wave(5, 2, 58) },
          { pid: 9310, name: "NVIDIA Broadcast.exe", vram: 0.7 * 2 ** 30, usage: 6 },
          { pid: 8120, name: "dwm.exe", vram: 0.4 * 2 ** 30, usage: 1.2 },
        ],
      },
    ],
  };
}

const internet = demoFriends();

export async function demoInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  await new Promise((r) => setTimeout(r, 60));
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case "get_overview":
      return r(structuredClone(overview));
    case "update_settings":
      Object.assign(config, args.patch);
      return r(structuredClone(overview));
    case "set_vcp_key":
      overview.has_vcp_key = true;
      return r({ saved: true, verified: true, message: "VCP accepted the key." });
    case "clear_vcp_key":
      overview.has_vcp_key = false;
      config.key_mode = "own";
      return r(structuredClone(overview));
    case "detect_vcp_key":
      overview.has_vcp_key = true;
      return r({ saved: true, verified: true, message: "Found VCP's key in its settings and stored it." });
    case "create_vcp_key":
      overview.has_vcp_key = true;
      return r(FAKE_VCP_KEY);
    case "get_access_key":
      return r(config.key_mode === "vcp" ? FAKE_VCP_KEY : accessKey);
    case "new_access_key":
      accessKey = `demo-${Math.random().toString(36).slice(2)}`.padEnd(32, "x");
      config.key_mode = "own";
      return r(accessKey);
    case "set_access_key":
      accessKey = String(args.key);
      return r(undefined);
    case "open_pairing":
      overview.status.info.pairing_open = true;
      return r({ code: "482913", secs_left: 300 });
    case "get_pairing":
      return r(overview.status.info.pairing_open ? { code: "482913", secs_left: 287 } : null);
    case "close_pairing":
      overview.status.info.pairing_open = false;
      return r(undefined);
    case "revoke_device":
      overview.status.devices = overview.status.devices.filter((d) => d.id !== args.id);
      emit("proxy://status", overview.status);
      return r(undefined);
    case "vcp_action":
      return r(args.action === "stop" ? "Voice Creator Pro is closed." : "Voice Creator Pro is up after 31 s.");
    case "firewall_check":
      return r(structuredClone(firewall));
    case "firewall_fix":
      Object.assign(firewall, {
        rule_ok: true,
        blocks: 0,
        allows: 1,
        rule: { enabled: true, action: "Allow", profile: "Private, Domain", program: firewall.program },
      });
      return r(structuredClone(firewall));
    case "plugins_view":
    case "plugins_rescan": {
      const eq = { format: "clap" as const, path: "C:/Program Files/Common Files/CLAP/Surge XT Effects.clap", id: "org.surge-synth-team.surge-xt-fx", name: "Surge XT Effects", vendor: "Surge Synth Team" };
      const pitch = { format: "vst3" as const, path: "C:/Program Files/Common Files/VST3/Graillon 3.vst3", id: "5653544731673367", name: "Graillon 3", vendor: "Auburn Sounds" };
      return { installed: [eq, pitch], scanning: false, missing: ["demon"], errors: {}, chains: { main: [{ id: "eq1", plugin: eq, enabled: true }], variants: [{ id: "chip", name: "Chipmunk", plugin: pitch, enabled: true, weight: 2 }, { id: "demon", name: "Demon", plugin: { ...pitch, id: "other", name: "Big Pitch" }, enabled: true, weight: 1 }] }, editors: true } as T;
    }
    case "plugins_open":
      return undefined as T;
    case "obs_links":
    case "obs_remove": {
      const links = new URLSearchParams(location.search).has("obs")
        ? [{ id: "p1", name: "Friend: Lena", channels: ["lenaplays"], online: true, listeners: 1, reading: true, status: { current: { display: "Mikey_TV", speech: "Mikey sagt: gute Runde!" }, queue: [], paused: false }, lan: ["http://192.168.178.25:8112/o/Xk3vQ9pLm2RtW7aZ"], internet: "https://mystream.duckdns.org:8112/o/Xk3vQ9pLm2RtW7aZ", internet_secure: true, securing: false }]
        : [];
      return { port: 8112, error: null, links } as T;
    }
    case "get_logs":
      return r(logs);
    case "watch_system":
      clearInterval(systemTimer);
      if (args.on) {
        emit("proxy://system", system());
        systemTimer = window.setInterval(() => emit("proxy://system", system()), 2000);
      }
      return r(undefined);
    case "check_update":
      return r(overview.status.update);
    case "install_update":
    case "reset_certificate":
      return r(structuredClone(overview));
    case "diagnostics":
      return r({ demo: true });
    case "internet_status":
      return r(await internet.status());
    case "set_internet":
      return r(await internet.set(args.patch as InternetPatch));
    case "test_internet":
      return r((await internet.test()).reachable);
    case "create_invite":
      return r(await internet.invite(String(args.name), Number(args.days), Number(args.charsPerDay), (args.address as string) ?? null));
    case "withdraw_invite":
      return r(await internet.withdraw(String(args.id)));
    case "revoke_guest":
      return r(await internet.revoke(String(args.id)));
  }
  throw new Error(`demo: unknown command ${cmd}`);
}

export function demoListen(event: string, cb: (p: unknown) => void) {
  let set = listeners.get(event);
  if (!set) listeners.set(event, (set = new Set()));
  set.add(cb);
  return () => set.delete(cb);
}
