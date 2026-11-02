<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Download, Spinner } from "@ttv-tts/ui/icons";
import { listen, packs, presets, type CatalogPack, type PresetDetail } from "../api";
import { host, loadEngine, run, store, toast } from "../store";

// Ready-made voices: download and import, no computing. For one built-in
// preset, or (without presetId) for several at once.
const props = defineProps<{ presetId?: string; catalog: CatalogPack[] }>();
const emit = defineEmits<{ close: [] }>();
const { t, locale } = useI18n();

const d = ref<PresetDetail | null>(null);
const bulk = computed(() => !props.presetId);
/** Bulk: the presets that have ready-made voices, in the presets page's order. */
const offered = computed(() => store.presets.filter((p) => props.catalog.some((c) => c.preset === p.id)));
const chosen = ref<string[]>([]);
const langs = ref<string[]>([]);
const upTo = ref(0);
const progress = ref<{ file: string; done: number; total: number } | null>(null);

const mine = computed(() => props.catalog.filter((p) => (bulk.value ? chosen.value.includes(p.preset) : p.preset === props.presetId)));
const languages = computed(() => [...new Set(mine.value.map((p) => p.lang))].sort());
const tiers = computed(() => [...new Set(mine.value.map((p) => p.to))].sort((a, b) => a - b));

/** All voices of a slice computed in its language already? (Bulk: judged by
 * the counts; voices that exist are skipped by the import anyway.) */
const have = (p: CatalogPack) =>
  bulk.value ? (store.presets.find((x) => x.id === p.preset)?.computed[p.lang] ?? 0) >= p.to : !!d.value && d.value.voices.slice(p.from - 1, p.to).every((v) => v.langs.includes(p.lang));
const wanted = computed(() => mine.value.filter((p) => langs.value.includes(p.lang) && p.to <= upTo.value && !have(p)));
const bytes = computed(() => wanted.value.reduce((n, p) => n + p.bytes, 0));
const voices = computed(() => wanted.value.reduce((n, p) => n + p.voices, 0));

let off: (() => void) | undefined;
onMounted(async () => {
  const ui = locale.value.slice(0, 2);
  if (bulk.value) {
    chosen.value = offered.value.map((p) => p.id);
    langs.value = languages.value.filter((l) => l === ui);
  } else {
    const r = await run("preset", () => presets.get(props.presetId!));
    if (!r) return emit("close");
    d.value = r;
    // the UI language, plus what this preset has in other languages already
    langs.value = languages.value.filter((l) => l === ui || r.computed[l]);
  }
  if (!langs.value.length) langs.value = languages.value.slice(0, 1);
  upTo.value = tiers.value[0] ?? 0;
  off = await listen<{ file: string; done: number; total: number }>("app://download", (p) => (progress.value = p));
});
onBeforeUnmount(() => off?.());

function toggle(l: string) {
  langs.value = langs.value.includes(l) ? langs.value.filter((x) => x !== l) : [...langs.value, l];
}
function togglePreset(id: string) {
  chosen.value = chosen.value.includes(id) ? chosen.value.filter((x) => x !== id) : [...chosen.value, id];
}
/** Which of the files is downloading now (bulk downloads take a while). */
const step = computed(() => (progress.value ? wanted.value.findIndex((p) => p.file === progress.value!.file) + 1 : 0));

async function start() {
  const files = wanted.value.map((p) => p.file);
  const jobs = await run("download", () => packs.download(files));
  progress.value = null;
  if (jobs) {
    toast("ok", t("downloads.started", { n: jobs.reduce((n, j) => n + j.total, 0) }));
    await loadEngine();
    emit("close");
  }
}
const mb = (b: number) => (b / 1048576).toFixed(b > 100 << 20 ? 0 : 1);
</script>

<template>
  <UiModal docs-page="packs" docs-anchor="download" :title="bulk ? t('downloads.allTitle') : d ? t('downloads.title', { name: d.name }) : t('common.loading')" width="max-w-lg" @close="emit('close')">
    <div v-if="d || bulk" class="space-y-5">
      <p class="muted text-sm">{{ t("downloads.intro") }}<UiTip :text="t('tips.downloads')" /></p>
      <div v-if="bulk">
        <div class="label">{{ t("downloads.presets") }} <span class="badge ml-1">{{ chosen.length }} / {{ offered.length }}</span>
          <button type="button" class="btn btn-ghost btn-sm ml-auto" @click="chosen = chosen.length === offered.length ? [] : offered.map((p) => p.id)">{{ chosen.length === offered.length ? t("downloads.none") : t("downloads.all") }}</button>
        </div>
        <div class="flex flex-wrap gap-1.5">
          <button v-for="p in offered" :key="p.id" type="button" class="chip" :class="chosen.includes(p.id) && 'chip-on'" @click="togglePreset(p.id)">{{ p.name }}</button>
        </div>
      </div>
      <div>
        <div class="label">{{ t("compute.languages") }}</div>
        <div class="flex flex-wrap gap-1.5">
          <button v-for="l in languages" :key="l" type="button" class="chip" :class="langs.includes(l) && 'chip-on'" @click="toggle(l)">{{ t(`lang.${l}`) }}</button>
        </div>
      </div>
      <div>
        <div class="label">{{ t("compute.voices") }}</div>
        <div class="flex flex-wrap gap-1.5">
          <button v-for="n in tiers" :key="n" type="button" class="chip" :class="upTo === n && 'chip-on'" @click="upTo = n">{{ t("downloads.first", { n }) }}</button>
        </div>
      </div>
      <div class="rounded-xl border border-line bg-panel-2/40 p-4 text-sm">
        <template v-if="voices">
          <span class="text-base font-semibold">{{ t("packs.voices", voices) }}</span>
          <span class="ml-2 text-muted">{{ mb(bytes) }} MB</span>
        </template>
        <span v-else class="text-muted">{{ t("downloads.haveAll") }}</span>
        <p v-if="voices > 600" class="mt-2 text-xs text-muted">{{ t("downloads.many") }}</p>
        <div v-if="progress" class="mt-2.5">
          <div class="bar bar-thin"><i :style="{ width: `${Math.round((progress.done / Math.max(1, progress.total)) * 100)}%` }" /></div>
          <span class="text-xs text-muted"><template v-if="wanted.length > 1">{{ step }} / {{ wanted.length }} · </template>{{ progress.file }} · {{ mb(progress.done) }} / {{ mb(progress.total) }} MB</span>
        </div>
      </div>
      <p v-if="!host?.connected" class="notice notice-warn text-sm">{{ t("presets.noHost") }}</p>
    </div>
    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.cancel") }}</button>
      <button class="btn btn-primary" :disabled="!voices || !host?.connected || store.busy === 'download'" @click="start">
        <Spinner v-if="store.busy === 'download'" :size="15" class="spin" /><Download v-else :size="15" />{{ t("downloads.start", voices) }}
      </button>
    </template>
  </UiModal>
</template>
