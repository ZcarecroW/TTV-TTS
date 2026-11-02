<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Check, Play, Refresh, Sparkle, Spinner, Stop, Waveform } from "@ttv-tts/ui/icons";
import { api, presets, type ReviewItem, type ReviewList } from "../api";
import { issueText } from "../quality";
import { host, loadEngine, loadPresets, run, store, toast } from "../store";

// Voices the quality check lists: listen, then keep, design again or improve.
const props = defineProps<{ presetId?: string | null }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

const list = ref<ReviewList | null>(null);
const text = ref("");
/** "<key>:new" or "<key>:ref" while playing */
const playing = ref<string | null>(null);

async function load() {
  const r = await presets.review().catch(() => null);
  if (r) list.value = r;
}
onMounted(load);
let timer = 0;
watch(
  () => store.library?.rev,
  () => {
    clearTimeout(timer);
    timer = window.setTimeout(load, 300);
  },
);
onBeforeUnmount(() => clearTimeout(timer));

const items = computed(() => (list.value?.items ?? []).filter((i) => !props.presetId || i.preset_id === props.presetId));
const connected = computed(() => !!host.value?.connected);

async function play(i: ReviewItem, what: "new" | "ref") {
  const id = `${i.key}:${what}`;
  if (playing.value) {
    await api.skip();
    if (playing.value === id) return void (playing.value = null);
  }
  playing.value = id;
  try {
    await run(`play-${id}`, async () => {
      if (what === "new") await presets.listen(i.preset_id, i.index, i.lang, text.value);
      else await presets.playReference(i.key);
    });
  } finally {
    if (playing.value === id) playing.value = null;
  }
}

async function keep(keys: string[]) {
  const n = await run("keep", () => presets.keep(keys));
  if (n != null) {
    toast("ok", keys.length > 1 ? t("quality.keptAll", { n }) : t("quality.kept"));
    await Promise.all([load(), loadPresets()]);
  }
}

async function improve(keys: string[]) {
  const jobs = await run("improve", () => presets.improve(keys));
  if (jobs) {
    toast("ok", t("quality.improving", { n: keys.length }));
    await loadEngine();
  }
}

async function redo(i: ReviewItem) {
  if (!confirm(t("presets.redoConfirm", { name: i.name, lang: t(`lang.${i.lang}`) }))) return;
  const job = await run(`redo-${i.key}`, () => presets.redo(i.preset_id, i.index, i.lang));
  if (job) {
    toast("ok", t("presets.redoQueued", { name: i.name }));
    await Promise.all([loadEngine(), load()]);
  }
}

async function checkNow() {
  const jobs = await run("checkQuality", () => presets.checkQuality(props.presetId ?? null));
  if (jobs) {
    toast("ok", t("quality.checkStarted", { n: jobs.reduce((n, j) => n + j.total, 0) }));
    await loadEngine();
  }
}

/** Voices of a quality check that's still running (they may join the list). */
const checking = computed(() => store.engine?.jobs.some((j) => (j.kind === "review" || j.kind === "improve") && ["queued", "running", "waiting"].includes(j.state)));
</script>

<template>
  <UiModal docs-page="presets" docs-anchor="quality" :title="t('quality.title')" width="max-w-4xl" @close="emit('close')">
    <div class="space-y-4">
      <p class="muted text-sm">{{ t("quality.intro") }}</p>

      <div class="grid gap-3 sm:grid-cols-[1fr_auto] sm:items-center">
        <input v-model="text" class="input" :placeholder="t('quality.text')" />
        <span class="flex gap-2">
          <button class="btn btn-sm" :disabled="!items.length || !connected || store.busy === 'improve'" :data-tip="t('quality.improveTip')" @click="improve(items.map((i) => i.key))">
            <Sparkle :size="14" />{{ t("quality.improveAll") }}
          </button>
          <button class="btn btn-sm" :disabled="!items.length || store.busy === 'keep'" @click="keep(items.map((i) => i.key))"><Check :size="14" />{{ t("quality.keepAll") }}</button>
        </span>
      </div>

      <p v-if="list && list.unchecked" class="notice notice-info text-sm">
        <span class="flex-1">{{ t("quality.unchecked", list.unchecked) }}<UiTip :text="t('quality.checkTip')" /></span>
        <button class="btn btn-sm btn-primary" :disabled="!connected || store.busy === 'checkQuality'" @click="checkNow">
          <Spinner v-if="store.busy === 'checkQuality'" :size="14" class="spin" />{{ t("quality.checkNow") }}
        </button>
      </p>

      <p v-if="!list" class="text-sm text-muted"><Spinner :size="14" class="spin" /> {{ t("common.loading") }}</p>
      <div v-else-if="!items.length" class="rounded-xl border border-line p-5 text-center text-sm text-muted">
        {{ t("quality.empty") }}
        <span v-if="list.checked" class="block text-xs">{{ t("quality.checkedInfo", { checked: list.checked, kept: list.kept }) }}</span>
        <span v-if="checking" class="mt-1 block text-xs"><Spinner :size="12" class="spin" /> {{ t("quality.stillChecking") }}</span>
      </div>
      <ul v-else class="divide-y divide-line rounded-xl border border-line">
        <li v-for="i in items" :key="i.key" data-review class="flex flex-wrap items-start gap-3 px-3.5 py-3">
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span class="font-medium">{{ i.name }}</span>
              <span class="badge">{{ i.lang }}</span>
              <span class="badge" :class="i.quality.score < 0.4 ? 'badge-err' : 'badge-warn'">{{ i.quality.score < 0.4 ? t("quality.clearly") : t("quality.maybe") }} · {{ Math.round(i.quality.score * 100) }}</span>
              <span class="text-xs text-muted">{{ i.preset_name }} · {{ t(`presets.gender.${i.gender}`) }} · {{ i.age }}</span>
            </div>
            <p class="mt-0.5 line-clamp-1 text-xs text-muted" :data-tip="i.description">{{ i.description }}</p>
            <ul class="mt-1 flex flex-wrap gap-1.5">
              <li v-for="(q, n) in i.quality.issues" :key="n" class="rounded-md bg-panel-2 px-1.5 py-0.5 text-xs">{{ issueText(t, q) }}</li>
            </ul>
          </div>
          <span class="flex shrink-0 gap-1">
            <button class="btn btn-sm btn-icon" :disabled="!connected" :aria-label="t('quality.sentence')" :data-tip="t('quality.sentence')" @click="play(i, 'new')">
              <Spinner v-if="playing === `${i.key}:new` && store.busy === `play-${i.key}:new`" :size="15" class="spin" />
              <Stop v-else-if="playing === `${i.key}:new`" :size="15" />
              <Play v-else :size="15" />
            </button>
            <button class="btn btn-sm btn-icon" :disabled="!connected" :aria-label="t('quality.original')" :data-tip="t('quality.originalTip')" @click="play(i, 'ref')">
              <Stop v-if="playing === `${i.key}:ref`" :size="15" />
              <Waveform v-else :size="15" />
            </button>
            <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('quality.keep')" :data-tip="t('quality.keepTip')" @click="keep([i.key])"><Check :size="15" /></button>
            <button class="btn btn-ghost btn-sm btn-icon" :disabled="!connected" :aria-label="t('presets.redo')" :data-tip="t('presets.redoTip')" @click="redo(i)"><Refresh :size="15" /></button>
            <button class="btn btn-ghost btn-sm btn-icon" :disabled="!connected" :aria-label="t('quality.improve')" :data-tip="t('quality.improveTip')" @click="improve([i.key])"><Sparkle :size="15" /></button>
          </span>
        </li>
      </ul>
    </div>
    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.close") }}</button>
    </template>
  </UiModal>
</template>
