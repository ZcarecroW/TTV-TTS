import { computed, reactive } from "vue";
import { api, errText, listen, type Config, type FirewallStatus, type Overview, type Pairing, type ProxyStatus, type SystemStats } from "./api";
import { i18n, setLocale } from "./i18n";
import { setTheme } from "@ttv-tts/ui/theme";

export type View = "overview" | "connect" | "vcp" | "firewall" | "log" | "settings";

export const store = reactive({
  ov: null as Overview | null,
  system: null as SystemStats | null,
  firewall: null as FirewallStatus | null,
  firewallChecking: false,
  pairing: null as Pairing | null,
  view: "overview" as View,
  toast: null as { kind: "ok" | "err" | "info"; text: string } | null,
  busy: "" as string,
});

let toastTimer = 0;
export function toast(kind: "ok" | "err" | "info", text: string) {
  store.toast = { kind, text };
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (store.toast = null), kind === "err" ? 7000 : 3500);
}

export const status = computed(() => store.ov?.status);
export const config = computed(() => store.ov?.config);

/** Runs a command, shows its error as a toast, marks it busy meanwhile. */
export async function run<T>(name: string, fn: () => Promise<T>): Promise<T | undefined> {
  store.busy = name;
  try {
    return await fn();
  } catch (e) {
    toast("err", errText(e));
    return undefined;
  } finally {
    if (store.busy === name) store.busy = "";
  }
}

export async function save(patch: Partial<Config>, quiet = false) {
  const ov = await run("save", () => api.updateSettings(patch));
  if (!ov) return false;
  store.ov = ov;
  if (patch.language) await setLocale(patch.language);
  if (patch.theme) setTheme(patch.theme);
  if (!quiet) toast("ok", i18n.global.t("connect.saved"));
  return true;
}

export async function refresh() {
  store.ov = await api.overview();
}

export async function checkFirewall() {
  store.firewallChecking = true;
  try {
    store.firewall = await api.firewallCheck();
  } finally {
    store.firewallChecking = false;
  }
}

export async function openPairing() {
  const p = await run("pair", () => api.openPairing());
  if (p) store.pairing = p;
}

export async function closePairing() {
  store.pairing = null;
  await api.closePairing();
}

export async function init() {
  await refresh();
  const c = store.ov!.config;
  setTheme(c.theme);
  await setLocale(c.language);
  await listen<ProxyStatus>("proxy://status", (s) => {
    if (store.ov) store.ov.status = s;
  });
  await listen<SystemStats>("proxy://system", (s) => (store.system = s));
  await listen<null>("proxy://pair", () => void openPairing());
  await listen<string>("proxy://paired", (name) => {
    store.pairing = null;
    toast("ok", i18n.global.t("connect.pairedToast", { name }));
  });
  void checkFirewall();
}

export interface Problem {
  text: string;
  action?: { label: string; run: () => void };
  kind: "err" | "warn" | "info";
}

/** What keeps the proxy from being useful, most important first. */
export const problems = computed<Problem[]>(() => {
  const t = i18n.global.t;
  const ov = store.ov;
  if (!ov) return [];
  const out: Problem[] = [];
  const v = ov.status.vcp;
  const go = (view: View) => () => (store.view = view);
  if (ov.listen_error) out.push({ kind: "err", text: t("status.listen", { port: ov.config.port }), action: { label: t("status.goTo"), run: go("connect") } });
  if (v.starting) out.push({ kind: "info", text: t("status.vcpStarting") });
  else if (!v.running && !v.reachable)
    out.push({
      kind: "err",
      text: t("status.vcpOff"),
      action: v.exe ? { label: t("overview.start"), run: () => void run("vcp", () => api.vcpAction("start")) } : { label: t("status.goTo"), run: go("vcp") },
    });
  else if (!v.reachable) out.push({ kind: "err", text: t("status.vcpNoApi"), action: { label: t("status.goTo"), run: go("vcp") } });
  else if (v.key_ok === false)
    out.push({ kind: "err", text: ov.has_vcp_key ? t("status.vcpKeyWrong") : t("status.vcpKey"), action: { label: t("status.fix"), run: go("vcp") } });
  const fw = store.firewall;
  if (fw?.managed && !fw.error && (fw.blocks > 0 || (!fw.rule_ok && fw.allows === 0) || fw.public_network_blocked))
    out.push({ kind: "warn", text: t("status.firewall"), action: { label: t("status.fix"), run: go("firewall") } });
  if (ov.status.devices.length === 0 && out.length === 0)
    out.push({ kind: "info", text: t("status.noDevices"), action: { label: t("overview.pair"), run: () => void openPairing() } });
  return out;
});
