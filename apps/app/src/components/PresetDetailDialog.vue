<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import { copyText } from "@ttv-tts/ui/clipboard";
import { Check, CircleCheck, Copy, Layers, Pen, Play, Refresh, Spinner, Sparkle, Stop, Trash, Upload, Warning } from "@ttv-tts/ui/icons";
import { api, DESIGN_LANGS, presets, type PresetDetail, type VoiceView } from "../api";
import { host, loadEngine, loadPresets, run, store, toast } from "../store";

const props = defineProps<{ presetId: string }>();
const emit = defineEmits<{ close: []; compute: [id: string]; edit: [id: string]; export: [id: string, keys: string[]] }>();
const { t } = useI18n();

const d = ref<PresetDetail | null>(null);
const filter = ref("");
const gender = ref<"" | "m" | "f" | "n">("");
const onlyReady = ref(false);
const onlyOdd = ref(false);
/** Selecting voices (for a pack of just those); keys "hash:lang". */
const selecting = ref(false);
const picked = reactive(new Set<string>());
function pick(v: VoiceView) {
  const k = `${v.hash}:${lang.value}`;
  if (picked.has(k)) picked.delete(k);
  else picked.add(k);
}
const lang = ref("en");
const text = ref("");
const playing = ref<number | null>(null);

/** "index:lang" of voices being designed again. */
const redoing = reactive(new Set<string>());

async function load(quiet = false) {
  const r = quiet ? await presets.get(props.presetId).catch(() => null) : await run("preset", () => presets.get(props.presetId));
  if (!r) return quiet ? undefined : emit("close");
  d.value = r;
  if (!quiet) {
    const langs = Object.entries(r.computed).sort((a, b) => b[1] - a[1]);
    if (langs.length && !r.computed[lang.value]) lang.value = langs[0]![0];
  }
  for (const k of [...redoing]) {
    const [i, l] = k.split(":");
    if (r.voices[Number(i)]?.langs.includes(l!)) redoing.delete(k);
  }
}
onMounted(() => load());
// Every library change (a job saved a voice, a voice was designed again or
// deleted) refreshes the list – debounced, a batch saves 16 voices in a row.
let reloadTimer = 0;
watch(
  () => store.library?.rev,
  () => {
    clearTimeout(reloadTimer);
    reloadTimer = window.setTimeout(() => load(true), 250);
  },
);
onBeforeUnmount(() => clearTimeout(reloadTimer));
// A redo that ended without a new voice (failed, cancelled) isn't "being designed" any more.
watch(
  () => store.engine?.jobs.some((j) => j.preset_id === props.presetId && ["queued", "running", "waiting", "paused"].includes(j.state)),
  (open) => {
    if (!open) redoing.clear();
  },
);

const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  return (d.value?.voices ?? []).filter(
    (v) =>
      (!gender.value || v.gender === gender.value) &&
      (!onlyReady.value || v.langs.length > 0) &&
      (!onlyOdd.value || v.odd.length > 0) &&
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

// A voice that came out different from its description: design it anew.
async function redo(v: VoiceView) {
  if (!d.value || !confirm(t("presets.redoConfirm", { name: v.name, lang: t(`lang.${lang.value}`) }))) return;
  const l = lang.value;
  const job = await run(`redo-${v.index}`, () => presets.redo(d.value!.id, v.index, l));
  if (job) {
    redoing.add(`${v.index}:${l}`);
    toast("ok", t("presets.redoQueued", { name: v.name }));
    await Promise.all([loadEngine(), load(true)]);
  }
}

async function checkQuality() {
  const jobs = await run("checkQuality", () => presets.checkQuality(props.presetId));
  if (jobs) {
    const n = jobs.reduce((n, j) => n + j.total, 0);
    toast(n ? "ok" : "info", n ? t("quality.checkStarted", { n }) : t("quality.allChecked"));
    await loadEngine();
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
        <button v-if="d.ready" type="button" class="chip" :class="selecting && 'chip-on'" :data-tip="t('packs.selectTip')" @click="(selecting = !selecting), picked.clear()"><Check :size="12" />{{ t("packs.select") }}</button>
        <button v-if="d.odd" type="button" class="chip" :class="onlyOdd && 'chip-on'" @click="onlyOdd = !onlyOdd"><Warning :size="12" />{{ t("quality.filterOdd") }} · {{ d.odd }}</button>
        <span class="ml-auto text-xs text-muted">{{ t("presets.shown", { n: shown.length, total: d.count }) }}</span>
      </div>

      <ul class="divide-y divide-line rounded-xl border border-line">
        <li v-for="v in shown" :key="v.index" class="flex items-start gap-3 px-3.5 py-2.5">
          <input
            v-if="selecting"
            type="checkbox"
            class="mt-1 size-4 shrink-0 accent-[var(--accent)]"
            :disabled="!v.langs.includes(lang)"
            :checked="picked.has(`${v.hash}:${lang}`)"
            :aria-label="t('packs.pick', { name: v.name })"
            @change="pick(v)"
          />
          <span class="w-8 shrink-0 pt-0.5 text-right font-mono text-xs text-muted">{{ v.index + 1 }}</span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span class="font-medium">{{ v.name }}</span>
              <span class="text-xs text-muted">{{ genderLabel(v.gender) }} · {{ v.age }}</span>
              <span v-if="v.fx" class="badge">{{ v.fx }}</span>
              <span class="ml-auto flex flex-wrap gap-1">
                <span v-if="redoing.has(`${v.index}:${lang}`)" class="badge badge-accent"><Spinner :size="12" class="spin" />{{ t("presets.redoing") }}</span>
                <span v-else-if="v.odd.includes(lang)" class="badge badge-warn" :data-tip="t('quality.oddRow')"><Warning :size="12" />{{ t("quality.maybe") }}</span>
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
          <button
            v-if="v.langs.includes(lang) && !redoing.has(`${v.index}:${lang}`)"
            class="btn btn-ghost btn-sm btn-icon shrink-0"
            :disabled="!canListen || store.busy === `redo-${v.index}`"
            :aria-label="t('presets.redo')"
            :data-tip="t('presets.redoTip')"
            @click="redo(v)"
          >
            <Refresh :size="15" />
          </button>
        </li>
      </ul>
    </div>

    <template #footer>
      <button class="btn" :disabled="!d" :data-tip="t('presets.exportTip')" @click="exportText"><Copy :size="15" />{{ t("presets.export") }}</button>
      <button class="btn" :disabled="!d || !d.ready || !canListen" :data-tip="t('packs.exportTip')" @click="emit('export', presetId, selecting ? [...picked] : [])">
        <Upload :size="15" />{{ selecting && picked.size ? t("packs.exportSelected", { n: picked.size }) : t("packs.export") }}
      </button>
      <button class="btn mr-auto" :disabled="!d || !d.ready || !canListen || store.busy === 'checkQuality'" :data-tip="t('quality.checkTip')" @click="checkQuality">
        <Spinner v-if="store.busy === 'checkQuality'" :size="15" class="spin" /><CircleCheck v-else :size="15" />{{ t("quality.checkPreset") }}
      </button>
      <button v-if="d && !d.builtin" class="btn btn-danger" @click="remove"><Trash :size="15" />{{ t("presets.delete") }}</button>
      <button class="btn" :disabled="!d" @click="d?.builtin ? duplicate() : emit('edit', presetId)">
        <Layers v-if="d?.builtin" :size="15" /><Pen v-else :size="15" />{{ d?.builtin ? t("presets.duplicate") : t("presets.edit") }}
      </button>
      <button class="btn btn-primary" :disabled="!d" @click="emit('compute', presetId)"><Sparkle :size="15" />{{ t("presets.compute") }}</button>
    </template>
  </UiModal>
</template>
