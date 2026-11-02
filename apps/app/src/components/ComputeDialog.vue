<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import { duration } from "@ttv-tts/ui/format";
import { Bolt, Gauge, Spinner, Warning } from "@ttv-tts/ui/icons";
import { DESIGN_LANGS, presets, TIERS, type PresetDetail } from "../api";
import { host, loadEngine, run, store, toast } from "../store";

const props = defineProps<{ presetId: string }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

const detail = ref<PresetDetail | null>(null);
const count = ref("30");
const langs = ref<string[]>([]);
const takes = ref("2");
const check = ref(true);
const backup = ref(true);
const remembered = (k: string) => {
  try {
    return localStorage.getItem(`ttv.compute.${k}`) === "1";
  } catch {
    return false;
  }
};
const quality = ref(remembered("quality"));
const improve = ref(remembered("improve"));
watch([quality, improve], ([q, i]) => {
  try {
    localStorage.setItem("ttv.compute.quality", q ? "1" : "0");
    localStorage.setItem("ttv.compute.improve", i ? "1" : "0");
  } catch {
    /* private mode */
  }
});
const eta = ref<number | null | undefined>(undefined);

onMounted(async () => {
  const d = await run("preset", () => presets.get(props.presetId));
  if (!d) return emit("close");
  detail.value = d;
  // Start with what was computed before, else English (+ the UI language).
  const before = Object.keys(d.computed);
  const ui = (store.app?.config.language ?? "en").slice(0, 2);
  langs.value = before.length ? before : [...new Set(["en", DESIGN_LANGS.includes(ui as never) ? ui : "en"])];
  // The smallest tier that isn't complete yet ("all" is value 0).
  const tier = TIERS.find((x) => x <= d.count && !d.voices.slice(0, x).every((v) => before.length && before.every((l) => v.langs.includes(l))));
  count.value = String(tier === undefined || tier >= d.count ? (d.count <= 30 ? 0 : (tier ?? 30)) : tier);
  if (Number(count.value) >= d.count) count.value = "0";
});

const tierOptions = computed(() => {
  const n = detail.value?.count ?? 0;
  return [...TIERS.filter((x) => x < n).map((x) => ({ value: String(x), label: String(x) })), { value: "0", label: t("compute.all", { n }) }];
});

/** Voices this job would really compute (the rest exists already). */
const todo = computed(() => {
  const d = detail.value;
  if (!d) return { add: 0, have: 0 };
  const n = Number(count.value) || d.count;
  let add = 0;
  let have = 0;
  for (const v of d.voices.slice(0, n)) for (const l of langs.value) v.langs.includes(l) ? have++ : add++;
  return { add, have };
});

let etaSeq = 0;
async function updateEta() {
  const seq = ++etaSeq;
  if (!todo.value.add) return (eta.value = 0);
  const e = await presets.estimate(todo.value.add, Number(takes.value), check.value).catch(() => null);
  // the quality check listens to every voice once more (~15 %)
  if (seq === etaSeq) eta.value = e == null ? e : quality.value ? Math.round(e * 1.15) : e;
}
watch([todo, takes, check, quality], updateEta);
watch(detail, updateEta);

async function measure() {
  const r = await run("benchmark", () => presets.benchmark());
  if (r != null) {
    toast("ok", t("compute.measured", { s: r.toFixed(1) }));
    await updateEta();
  }
}

const long = computed(() => (eta.value ?? 0) > 2 * 3600);
const direct = computed(() => host.value?.kind === "direct");

function toggle(l: string) {
  langs.value = langs.value.includes(l) ? langs.value.filter((x) => x !== l) : [...langs.value, l];
}

async function start() {
  const d = detail.value;
  if (!d) return;
  const job = await run("compute", () =>
    presets.compute({ preset_id: d.id, count: Number(count.value), languages: langs.value, options: { takes: Number(takes.value), speech_check: check.value, backup: backup.value, quality: quality.value, improve: quality.value && improve.value } }),
  );
  if (job) {
    toast("ok", t("compute.started", { n: job.total }));
    await loadEngine();
    emit("close");
  }
}
</script>

<template>
  <UiModal docs-page="presets" docs-anchor="compute" :title="detail ? t('compute.title', { name: detail.name }) : t('common.loading')" width="max-w-xl" @close="emit('close')">
    <div v-if="detail" class="space-y-5">
      <UiSegmented v-model="count" :options="tierOptions" :label="t('compute.voices')" :tip="t('tips.computeVoices')" />

      <div>
        <div class="label">{{ t("compute.languages") }}<UiTip :text="t('tips.computeLanguages')" /></div>
        <div class="flex flex-wrap gap-1.5">
          <button v-for="l in DESIGN_LANGS" :key="l" type="button" class="chip" :class="langs.includes(l) && 'chip-on'" :aria-pressed="langs.includes(l)" @click="toggle(l)">
            {{ t(`lang.${l}`) }}<span v-if="detail.computed[l]" class="opacity-70">{{ detail.computed[l] }}</span>
          </button>
        </div>
      </div>

      <div class="grid gap-4 sm:grid-cols-2">
        <UiSegmented
          v-model="takes"
          :options="[
            { value: '1', label: '1' },
            { value: '2', label: '2' },
            { value: '3', label: '3' },
          ]"
          :label="t('compute.takes')"
          :tip="t('tips.takes')"
        />
        <div class="space-y-2 pt-1">
          <UiToggle v-model="check" :label="t('compute.check')" :tip="t('tips.speechCheck')" />
          <UiToggle v-model="backup" :label="t('compute.backup')" :tip="t('tips.backup')" />
        </div>
      </div>

      <div class="space-y-2 rounded-xl border border-line p-3.5">
        <UiToggle v-model="quality" :label="t('compute.quality')" :tip="t('tips.qualityCheck')" />
        <UiToggle v-model="improve" :disabled="!quality" :label="t('compute.improve')" :tip="t('tips.improveAuto')" />
      </div>

      <div class="rounded-xl border border-line bg-panel-2/40 p-4 text-sm">
        <div class="flex flex-wrap items-baseline gap-x-2">
          <span class="text-base font-semibold">{{ t("compute.new", todo.add) }}</span>
          <span v-if="todo.have" class="text-muted">{{ t("compute.have", { n: todo.have }) }}</span>
        </div>
        <div class="mt-1.5 flex flex-wrap items-center gap-2 text-muted">
          <Gauge :size="16" />
          <template v-if="eta === undefined"><Spinner :size="14" class="spin" />{{ t("common.loading") }}</template>
          <template v-else-if="eta === null">
            {{ t("compute.unmeasured") }}
            <button class="btn btn-sm" :disabled="store.busy === 'benchmark'" @click="measure">
              <Spinner v-if="store.busy === 'benchmark'" :size="14" class="spin" /><Bolt v-else :size="14" />{{ store.busy === "benchmark" ? t("compute.measuring") : t("compute.measure") }}
            </button>
          </template>
          <template v-else-if="todo.add">{{ t("compute.eta", { time: duration(eta) }) }}<UiTip :text="t('tips.eta')" /></template>
          <template v-else>{{ t("compute.nothing") }}</template>
        </div>
        <p v-if="long" class="mt-2.5 flex gap-2 text-warn"><Warning :size="16" class="mt-0.5" />{{ direct ? t("compute.longDirect") : t("compute.longProxy") }}</p>
      </div>
    </div>

    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.cancel") }}</button>
      <button class="btn btn-primary" :disabled="!detail || !langs.length || !todo.add || store.busy === 'compute'" @click="start">
        <Spinner v-if="store.busy === 'compute'" :size="15" class="spin" />{{ t("compute.start") }}
      </button>
    </template>
  </UiModal>
</template>
