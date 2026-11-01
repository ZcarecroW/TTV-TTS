import { computed, reactive } from "vue";
import { api, errText, listen, reader, updates, type AppView, type HostStatus, type ItemView, type ReaderStatus, type ReaderView, type SystemStats, type UpdateInfo } from "./api";
import { i18n, setLocale } from "./i18n";
import { setTheme } from "@ttv-tts/ui/theme";

export type View = "live" | "profiles" | "voices" | "host" | "settings";

export const store = reactive({
  app: null as AppView | null,
  hostSystem: null as SystemStats | null,
  reader: null as ReaderView | null,
  update: null as UpdateInfo | null,
  view: "live" as View,
  toast: null as { kind: "ok" | "err" | "info"; text: string } | null,
  busy: "" as string,
});

export const config = computed(() => store.app?.config);
export const host = computed(() => store.app?.host);

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

export async function init() {
  await apply(await api.state());
  await listen<HostStatus>("app://host", (h) => {
    if (store.app) store.app.host = h;
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
}

export const t = (key: string, values?: Record<string, unknown>) => i18n.global.t(key, values ?? {});
