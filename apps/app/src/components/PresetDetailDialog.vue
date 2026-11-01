<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import { copyText } from "@ttv-tts/ui/clipboard";
import { Copy, Layers, Pen, Play, Spinner, Sparkle, Stop, Trash } from "@ttv-tts/ui/icons";
import { api, DESIGN_LANGS, presets, type PresetDetail, type VoiceView } from "../api";
import { host, loadPresets, run, store, toast } from "../store";

const props = defineProps<{ presetId: string }>();
const emit = defineEmits<{ close: []; compute: [id: string]; edit: [id: string] }>();
const { t } = useI18n();

const d = ref<PresetDetail | null>(null);
const filter = ref("");
const gender = ref<"" | "m" | "f" | "n">("");
const onlyReady = ref(false);
const lang = ref("en");
const text = ref("");
const playing = ref<number | null>(null);

async function load() {
  const r = await run("preset", () => presets.get(props.presetId));
  if (!r) return emit("close");
  d.value = r;
  const langs = Object.entries(r.computed).sort((a, b) => b[1] - a[1]);
  if (langs.length && !r.computed[lang.value]) lang.value = langs[0]![0];
}
onMounted(load);
// Counts change while a job computes this preset.
watch(() => store.presets.find((p) => p.id === props.presetId)?.ready, load);

const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  return (d.value?.voices ?? []).filter(
    (v) =>
      (!gender.value || v.gender === gender.value) &&
      (!onlyReady.value || v.langs.length > 0) &&
      (!f || v.name.toLowerCase().includes(f) || v.description.toLowerCase().includes(f)),
  );
});

const canListen = computed(() => !!host.value?.connected);

async function listen(v: VoiceView) {
  if (!d.value) return;
  if (playing.value !== null) await api.skip();
  playing.value = v.index;
  try {
    await run("listen", () => presets.listen(d.value!.id, v.index, lang.value, text.value));
  } finally {
    if (playing.value === v.index) playing.value = null;
  }
}

async function stop() {
  await api.skip();
  playing.value = null;
}

async function exportText() {
  const s = await run("export", () => presets.exportText(props.presetId));
  if (s && (await copyText(s))) toast("ok", t("presets.exported"));
}

async function duplicate() {
  const p = await run("duplicate", () => presets.duplicate(props.presetId));
  if (p) {
    await loadPresets();
    toast("ok", t("presets.duplicated", { name: p.name }));
    emit("edit", p.id);
  }
}

async function remove() {
  if (!d.value || !confirm(t("presets.deleteConfirm", { name: d.value.name }))) return;
  await run("delete", () => presets.remove(props.presetId));
  await loadPresets();
  emit("close");
}

const genderLabel = (g: string) => t(`presets.gender.${g}`);
</script>

<template>
  <UiModal :title="d?.name ?? t('common.loading')" width="max-w-4xl" @close="emit('close')">
    <div v-if="d" class="space-y-4">
      <p class="muted text-sm">{{ d.description }}</p>

      <div class="rounded-xl border border-line bg-panel-2/40 p-3.5">
        <div class="grid gap-3 sm:grid-cols-[1fr_auto]">
          <input v-model="text" class="input" :placeholder="t('presets.listenPlaceholder')" />
          <select v-model="lang" class="input sm:w-44">
            <option v-for="l in DESIGN_LANGS" :key="l" :value="l">{{ t(`lang.${l}`) }} · {{ d.computed[l] ?? 0 }}</option>
          </select>
        </div>
        <p class="hint mt-2">{{ canListen ? t("presets.listenHint") : t("voices.noHost") }}</p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <input v-model="filter" class="input max-w-64" :placeholder="t('presets.filter')" />
        <button v-for="g in ['', 'm', 'f', 'n'] as const" :key="g" type="button" class="chip" :class="gender === g && 'chip-on'" @click="gender = g">
          {{ g ? genderLabel(g) : t("presets.everyone") }}
        </button>
        <button type="button" class="chip" :class="onlyReady && 'chip-on'" @click="onlyReady = !onlyReady">{{ t("presets.onlyReady") }}</button>
        <span class="ml-auto text-xs text-muted">{{ t("presets.shown", { n: shown.length, total: d.count }) }}</span>
      </div>

      <ul class="divide-y divide-line rounded-xl border border-line">
        <li v-for="v in shown" :key="v.index" class="flex items-start gap-3 px-3.5 py-2.5">
          <span class="w-8 shrink-0 pt-0.5 text-right font-mono text-xs text-muted">{{ v.index + 1 }}</span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span class="font-medium">{{ v.name }}</span>
              <span class="text-xs text-muted">{{ genderLabel(v.gender) }} · {{ v.age }}</span>
              <span v-if="v.fx" class="badge">{{ v.fx }}</span>
              <span class="ml-auto flex flex-wrap gap-1">
                <span v-for="l in v.langs" :key="l" class="badge" :class="l === lang && 'badge-ok'">{{ l }}</span>
              </span>
            </div>
            <p class="mt-0.5 text-sm text-muted">{{ v.description }}</p>
          </div>
          <button
            class="btn btn-sm btn-icon shrink-0"
            :disabled="!canListen || !v.langs.includes(lang)"
            :aria-label="t('presets.listen')"
            :data-tip="v.langs.includes(lang) ? t('presets.listen') : t('presets.notComputed', { lang: t(`lang.${lang}`) })"
            @click="playing === v.index ? stop() : listen(v)"
          >
            <Spinner v-if="playing === v.index && store.busy === 'listen'" :size="15" class="spin" />
            <Stop v-else-if="playing === v.index" :size="15" />
            <Play v-else :size="15" />
          </button>
        </li>
      </ul>
    </div>

    <template #footer>
      <button class="btn mr-auto" :disabled="!d" @click="exportText"><Copy :size="15" />{{ t("presets.export") }}</button>
      <button v-if="d && !d.builtin" class="btn btn-danger" @click="remove"><Trash :size="15" />{{ t("presets.delete") }}</button>
      <button class="btn" :disabled="!d" @click="d?.builtin ? duplicate() : emit('edit', presetId)">
        <Layers v-if="d?.builtin" :size="15" /><Pen v-else :size="15" />{{ d?.builtin ? t("presets.duplicate") : t("presets.edit") }}
      </button>
      <button class="btn btn-primary" :disabled="!d" @click="emit('compute', presetId)"><Sparkle :size="15" />{{ t("presets.compute") }}</button>
    </template>
  </UiModal>
</template>
