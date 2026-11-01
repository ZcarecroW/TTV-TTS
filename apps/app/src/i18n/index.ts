import { createI18n } from "vue-i18n";
import type en from "./en";

export type Locale = "en" | "de";
type Messages = typeof en;

export function detectLocale(setting: string): Locale {
  if (setting === "en" || setting === "de") return setting;
  return navigator.language.toLowerCase().startsWith("de") ? "de" : "en";
}

// Only the language in use is loaded; both have the same keys (de.ts is
// typed against en.ts), so nothing needs a fallback.
const loaders: Record<Locale, () => Promise<{ default: Messages }>> = {
  en: () => import("./en"),
  de: () => import("./de"),
};

export const i18n = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages: {} as Record<Locale, Messages>,
});

export async function setLocale(setting: string) {
  const l = detectLocale(setting);
  if (!i18n.global.availableLocales.includes(l)) i18n.global.setLocaleMessage(l, (await loaders[l]()).default);
  i18n.global.locale.value = l;
  document.documentElement.lang = l;
}
