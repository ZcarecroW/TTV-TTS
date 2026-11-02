<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Download, Gear, Microphone, Play, Refresh, Spinner, Warning } from "@ttv-tts/ui/icons";
import { DESIGN_LANGS, osVoices, type OsVoice, type OsVoicesView } from "../api";
import { run, store, toast } from "../store";

// The system's own voices, the fallback when the voice host can't speak:
// one per language, speed and pitch, try them, install more.
const { t } = useI18n();
const view = ref<OsVoicesView | null>(null);
const load = async () => (view.value = (await osVoices.list().catch(() => null)) ?? view.value);
onMounted(load);

const ofLang = (lang: string) => (view.value?.voices ?? []).filter((v) => v.language.toLowerCase().split(/[-_]/)[0] === lang);
/** The voice that reads a language now (chosen, else the first of it). */
function current(lang: string): OsVoice | null {
  const v = view.value;
  if (!v) return null;
  const id = v.config.voices[lang];
  return v.voices.find((x) => x.id === id) ?? ofLang(lang)[0] ?? null;
}
const pack = (lang: string) => view.value?.packs.find(([l]) => l === lang)?.[1] ?? null;

async function save() {
  if (!view.value) return;
  const v = await run("osvoices", () => osVoices.set(view.value!.config));
  if (v) view.value = v;
}
function choose(lang: string, id: string) {
  if (!view.value) return;
  if (id) view.value.config.voices[lang] = id;
  else delete view.value.config.voices[lang];
  void save();
}

const text = ref("");
async function tryLang(lang: string) {
  const v = current(lang);
  const say = text.value.trim() || t(`settings.osVoiceSample.${lang}`);
  const r = await run("osvoice-try", () => osVoices.try(v?.id ?? null, lang, say, view.value?.config));
  if (r) toast("ok", t("settings.osVoiceTried", { secs: r.secs.toFixed(1), ms: r.ms }));
}
const installing = ref("");
async function install(lang: string) {
  const locale = pack(lang);
  if (!locale) return;
  installing.value = lang;
  toast("info", t("settings.osVoiceInstalling", { lang: t(`lang.${lang}`) }));
  const v = await run("osvoice-install", () => osVoices.install(locale));
  installing.value = "";
  if (v) {
    view.value = v;
    toast(ofLang(lang).length ? "ok" : "info", ofLang(lang).length ? t("settings.osVoiceInstalled", { lang: t(`lang.${lang}`), n: ofLang(lang).length }) : t("settings.osVoiceInstalledLater"));
  }
}
const missing = computed(() => DESIGN_LANGS.filter((l) => !ofLang(l).length));
</script>

<template>
  <section class="card p-5 md:col-span-2" data-os-voices>
    <div class="card-head">
      <Microphone />{{ t("settings.osVoiceTitle") }}<UiTip :text="t('tips.osVoices')" />
      <UiDocs class="ml-auto" page="settings" anchor="system-voice" />
    </div>
    <p class="mb-3 text-sm text-muted">{{ t("settings.osVoiceText") }}</p>
    <p v-if="!view" class="text-sm text-muted"><Spinner :size="14" class="spin" /> {{ t("common.loading") }}</p>
    <template v-else>
      <p v-if="!view.voices.length" class="notice notice-err mb-3 text-sm">{{ t(`settings.osVoiceNone.${view.os}`) }}</p>
      <div class="grid gap-x-6 gap-y-1.5 md:grid-cols-2">
        <div v-for="lang in DESIGN_LANGS" :key="lang" class="flex items-center gap-2 text-sm" :data-os-lang="lang">
          <span class="w-20 shrink-0 text-muted">{{ t(`lang.${lang}`) }}</span>
          <template v-if="ofLang(lang).length">
            <select class="input h-8 min-w-0 flex-1 py-0 text-sm" :value="view.config.voices[lang] ?? ''" :aria-label="t(`lang.${lang}`)" @change="choose(lang, ($event.target as HTMLSelectElement).value)">
              <option value="">{{ t("settings.osVoiceAuto", { name: ofLang(lang)[0]!.name }) }}</option>
              <option v-for="v in ofLang(lang)" :key="v.id" :value="v.id">{{ v.name }}{{ v.gender ? ` (${t(`settings.osVoiceGender.${v.gender}`)})` : "" }}</option>
            </select>
          </template>
          <span v-else class="flex min-w-0 flex-1 items-center gap-1.5 text-warn"><Warning :size="14" class="shrink-0" />{{ t("settings.osVoiceMissing") }}</span>
          <button v-if="!ofLang(lang).length && view.os === 'windows' && pack(lang)" class="btn btn-sm" :disabled="!!installing" :data-tip="t('settings.osVoiceInstallTip')" @click="install(lang)">
            <Spinner v-if="installing === lang" :size="14" class="spin" /><Download v-else :size="14" />{{ t("settings.osVoiceInstall") }}
          </button>
          <button class="btn btn-ghost btn-sm btn-icon" :disabled="store.busy === 'osvoice-try'" :aria-label="t('settings.osVoiceTry')" :data-tip="t('settings.osVoiceTry')" @click="tryLang(lang)"><Play :size="14" /></button>
        </div>
      </div>
      <p v-if="missing.length" class="hint mt-2">{{ t(`settings.osVoiceMissingHint.${view.os}`) }}</p>

      <div class="mt-4 grid gap-4 md:grid-cols-3">
        <UiField :label="t('settings.osVoiceRate', { n: view.config.rate.toFixed(2) })">
          <input v-model.number="view.config.rate" type="range" min="0.5" max="2" step="0.05" class="w-full" @change="save" />
        </UiField>
        <UiField :label="t('settings.osVoicePitch', { n: view.config.pitch.toFixed(2) })" :tip="t('tips.osVoicePitch')">
          <input v-model.number="view.config.pitch" type="range" min="0.5" max="1.5" step="0.05" class="w-full" @change="save" />
        </UiField>
        <UiField :label="t('settings.osVoiceText2')">
          <input v-model="text" class="input" :placeholder="t('settings.osVoiceTextPlaceholder')" />
        </UiField>
      </div>
      <div class="mt-3 flex flex-wrap gap-2">
        <button class="btn btn-sm" @click="run('osvoice-settings', () => osVoices.openSettings().then(() => true))"><Gear :size="14" />{{ t(`settings.osVoiceSettings.${view.os}`) }}</button>
        <button class="btn btn-ghost btn-sm" @click="load"><Refresh :size="14" />{{ t("settings.osVoiceRefresh") }}</button>
      </div>
    </template>
  </section>
</template>
