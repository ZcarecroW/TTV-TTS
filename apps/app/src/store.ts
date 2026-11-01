import { computed, reactive } from "vue";
import {
  api,
  errText,
  listen,
  presets,
  reader,
  updates,
  type AppView,
  type EngineStatus,
  type HostStatus,
  type ItemView,
  type JobView,
  type LibraryNotice,
  type PresetView,
  type ReaderStatus,
  type ReaderView,
  type SystemStats,
  type UpdateInfo,
} from "./api";
import { i18n, setLocale } from "./i18n";
import { setTheme } from "@ttv-tts/ui/theme";

export type View = "live" | "profiles" | "viewers" | "presets" | "voices" | "host" | "settings";

export const store = reactive({
  app: null as AppView | null,
  hostSystem: null as SystemStats | null,
  reader: null as ReaderView | null,
  update: null as UpdateInfo | null,
  presets: [] as PresetView[],
  engine: null as EngineStatus | null,
  library: null as LibraryNotice | null,
  /** Why jobs can't be loaded (e.g. a proxy too old for presets). */
  engineError: "" as string,
  view: "live" as View,
  toast: null as { kind: "ok" | "err" | "info"; text: string } | null,
  busy: "" as string,
});

export const config = computed(() => store.app?.config);
export const host = computed(() => store.app?.host);
/** Jobs that still work or wait. */
export const activeJobs = computed(() => store.engine?.jobs.filter((j) => !["done", "cancelled", "failed"].includes(j.state)) ?? []);

let toastTimer = 0;
export function toast(kind: "ok" | "err" | "info", text: string) {
  store.toast = { kind, text };
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (store.toast = null), kind === "err" ? 7000 : 3500);
}

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

/** Applies a fresh AppView (language/theme follow the settings). */
export async function apply(v: AppView | undefined) {
  if (!v) return;
  const before = store.app?.config;
  store.app = v;
  if (before?.language !== v.config.language) await setLocale(v.config.language);
  if (before?.theme !== v.config.theme) setTheme(v.config.theme);
}

/** Reloads the preset list (counts of computed voices). */
export async function loadPresets() {
  try {
    store.presets = await presets.list();
  } catch (e) {
    toast("err", errText(e));
  }
}

/** Reloads jobs and speed; quiet when no host is connected. */
export async function loadEngine() {
  try {
    store.engine = await presets.engine();
    store.engineError = "";
  } catch (e) {
    store.engine = null;
    store.engineError = errText(e);
  }
}

// Library changes arrive every few seconds while a job runs: refresh the
// preset counts at most every 2 s.
let presetTimer = 0;
function presetsSoon() {
  if (presetTimer) return;
  presetTimer = window.setTimeout(() => {
    presetTimer = 0;
    void loadPresets();
  }, 2000);
}

function onJob(job: JobView) {
  if (!store.engine) {
    void loadEngine();
    return;
  }
  const jobs = store.engine.jobs;
  const i = jobs.findIndex((j) => j.id === job.id);
  if (i >= 0) jobs[i] = job;
  else jobs.push(job);
}

export async function init() {
  await apply(await api.state());
  await listen<HostStatus>("app://host", (h) => {
    const was = store.app?.host.connected;
    if (store.app) store.app.host = h;
    if (h.connected && !was) void loadEngine();
  });
  await listen<SystemStats>("app://host-system", (s) => (store.hostSystem = s));
  store.update = await updates.info();
  await listen<UpdateInfo>("app://update", (u) => (store.update = u));
  store.reader = await reader.get();
  await listen<ReaderStatus>("app://reader", (s) => {
    if (store.reader) store.reader.status = s;
  });
  await listen<ItemView>("app://history", (item) => {
    const h = store.reader?.history;
    if (!h) return;
    const i = h.findIndex((x) => x.id === item.id);
    if (i >= 0) h[i] = item;
    else {
      h.push(item);
      if (h.length > 150) h.splice(0, h.length - 150);
    }
  });
  await listen<JobView>("app://job", onJob);
  await listen<LibraryNotice>("app://library", (n) => {
    store.library = n;
    presetsSoon();
  });
  void loadPresets();
  if (store.app?.host.connected) void loadEngine();
}

export const t = (key: string, values?: Record<string, unknown>) => i18n.global.t(key, values ?? {});
