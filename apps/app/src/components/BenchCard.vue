<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import { duration } from "@ttv-tts/ui/format";
import { Gauge, Play, Spinner, Stop } from "@ttv-tts/ui/icons";
import { presets, type BenchLevel, type BenchReport, type BenchSeries } from "../api";
import { loadEngine, run, store, toast } from "../store";

const { t, locale } = useI18n();

const level = ref<BenchLevel>("standard");
const reports = ref<BenchReport[]>([]);
const restartSlow = ref<boolean | null>(null);
const progress = computed(() => store.engine?.bench ?? null);
const running = computed(() => !!progress.value?.running);
const last = computed(() => reports.value[0] ?? null);
const pct = computed(() => (progress.value?.steps ? Math.round((progress.value.step / progress.value.steps) * 100) : 0));

const levels = computed(() => (["quick", "standard", "thorough"] as BenchLevel[]).map((value) => ({ value, label: t(`bench.level.${value}`) })));

async function history() {
  reports.value = (await presets.benchHistory().catch(() => ({ reports: [] }))).reports;
}

// The page follows the benchmark while it runs.
let timer = 0;
onMounted(async () => {
  await loadEngine();
  await history();
  restartSlow.value = (await presets.vcpSettings().catch(() => null))?.restart_when_slow ?? null;
  timer = window.setInterval(() => {
    if (running.value) void loadEngine();
  }, 1500);
});
onBeforeUnmount(() => clearInterval(timer));
watch(running, (now, before) => {
  if (before && !now) {
    void history();
    const error = progress.value?.error;
    if (error && error !== "stopped") toast("err", t("bench.failed", { error }));
    else if (!error) toast("ok", t("bench.finished"));
  }
});

async function start() {
  await run("bench", () => presets.benchStart({ level: level.value, restart: true }));
  await loadEngine();
}
async function stop() {
  await run("bench", () => presets.benchStop());
  await loadEngine();
}
async function setRestart(on: boolean) {
  restartSlow.value = on;
  await run("benchSettings", () => presets.setVcpSettings({ restart_when_slow: on }));
}
const restartModel = computed({ get: () => !!restartSlow.value, set: (x: boolean) => void setRestart(x) });

const secs = (s: number) => `${s.toFixed(s < 10 ? 2 : 1)} s`;
const ofKind = (list: BenchSeries[], kind: string) => list.find((s) => s.kind === kind);
const kinds = ["design", "speak", "listen"];
const rows = computed(() => {
  const r = last.value;
  if (!r) return [];
  return kinds.map((kind) => ({ kind, now: ofKind(r.series, kind), before: ofKind(r.before, kind) })).filter((x) => x.now);
});
/** How much a restart gave: the slowest kind of work, before against fresh. */
const gain = computed(() => {
  const all = rows.value.filter((x) => x.before?.median_secs && x.now?.median_secs).map((x) => x.before!.median_secs / x.now!.median_secs);
  return all.length ? Math.max(...all) : null;
});
/** What slowed down the most; getting faster isn't worth a word. */
const worstDrift = computed(() => {
  const d = last.value?.drift ?? [];
  return d.length ? d.reduce((a, b) => (b.percent > a.percent ? b : a)) : null;
});
const when = computed(() => (last.value ? new Date(last.value.at * 1000).toLocaleString(locale.value, { dateStyle: "medium", timeStyle: "short" }) : ""));
const expensive = computed(() => [...(last.value?.switches ?? [])].sort((a, b) => b.secs - a.secs));
const lowVram = computed(() => last.value?.variants.filter((v) => !v.asr_model) ?? []);
const lighter = computed(() => last.value?.variants.find((v) => v.asr_model) ?? null);
const percent = (x: number | null) => (x == null ? "–" : `${(x * 100).toFixed(1)} %`);
</script>

<template>
  <section class="card p-5">
    <div class="card-head"><Gauge />{{ t("bench.title") }}<UiDocs class="ml-auto" page="presets" anchor="benchmark" /></div>
    <p class="muted text-sm">{{ t("bench.hint") }}</p>

    <div class="mt-3 flex flex-wrap items-end gap-3">
      <UiSegmented v-model="level" :options="levels" :label="t('bench.intensity')" :disabled="running" />
      <button v-if="!running" class="btn btn-primary" :disabled="store.busy === 'bench'" @click="start"><Play :size="15" />{{ t("bench.start") }}</button>
      <button v-else class="btn" :disabled="store.busy === 'bench'" @click="stop"><Stop :size="15" />{{ t("bench.stop") }}</button>
    </div>
    <p class="hint">{{ t(`bench.about.${level}`) }}</p>

    <div v-if="running" class="mt-3" role="status">
      <div class="flex items-center gap-2 text-sm"><Spinner :size="14" class="spin" />{{ t(`bench.stage.${progress?.stage || "wait"}`) }}<span class="ml-auto text-xs text-muted">{{ pct }} %</span></div>
      <div class="bar mt-1.5"><i :style="{ width: `${pct}%` }" /></div>
      <p class="hint">{{ t("bench.exclusive") }}</p>
    </div>

    <template v-if="last && !running">
      <div class="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <div>
          <div class="label">{{ t("bench.perVoice") }}</div>
          <div class="text-lg font-semibold">{{ secs(last.advice.voice_secs) }}</div>
        </div>
        <div>
          <div class="label">{{ t("bench.batch") }}</div>
          <div class="text-lg font-semibold">{{ last.advice.batch_voices }}</div>
        </div>
        <div>
          <div class="label">{{ t("bench.switching") }}</div>
          <div class="text-lg font-semibold">{{ last.advice.switch_percent.toFixed(1) }} %</div>
        </div>
        <div v-if="last.vram_free_mb != null">
          <div class="label">{{ t("bench.vramFree") }}</div>
          <div class="text-lg font-semibold">{{ (last.vram_free_mb / 1024).toFixed(1) }} GB</div>
        </div>
      </div>

      <table class="mt-4 w-full text-sm">
        <thead>
          <tr class="text-left text-xs text-muted">
            <th class="py-1 font-medium">{{ t("bench.work") }}</th>
            <th class="py-1 text-right font-medium">{{ t("bench.fresh") }}</th>
            <th class="py-1 text-right font-medium">{{ t("bench.before") }}</th>
            <th class="py-1 text-right font-medium">{{ t("bench.worst") }}</th>
            <th class="py-1 text-right font-medium">{{ t("bench.load") }}</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-line/50">
          <tr v-for="r in rows" :key="r.kind">
            <td class="py-1">{{ t(`bench.kind.${r.kind}`) }}</td>
            <td class="py-1 text-right font-medium">{{ secs(r.now!.median_secs) }}</td>
            <td class="py-1 text-right text-muted">{{ r.before ? secs(r.before.median_secs) : "–" }}</td>
            <td class="py-1 text-right text-muted">{{ secs(r.now!.worst_secs) }}</td>
            <td class="py-1 text-right text-muted">{{ r.now!.first_secs != null ? secs(r.now!.first_secs) : "–" }}</td>
          </tr>
        </tbody>
      </table>

      <ul class="mt-3 space-y-1 text-sm">
        <li v-if="gain != null">{{ gain >= 1.25 ? t("bench.gainBig", { x: gain.toFixed(1) }) : t("bench.gainNone") }}</li>
        <li v-if="worstDrift">{{ worstDrift.percent >= 15 ? t("bench.driftBad", { kind: t(`bench.kind.${worstDrift.kind}`), n: Math.round(worstDrift.percent) }) : t("bench.driftOk") }}</li>
        <li v-if="expensive.length">{{ t("bench.switchWorst", { from: t(`bench.kind.${expensive[0].from}`), to: t(`bench.kind.${expensive[0].to}`), s: expensive[0].secs.toFixed(1) }) }}</li>
        <li v-for="v in lowVram" :key="String(v.low_vram)">{{ v.error ? t("bench.variantFailed") : t("bench.variant", { s: v.voice_secs.toFixed(1), now: last.advice.voice_secs.toFixed(1) }) }}</li>
        <li v-if="lighter">{{ lighter.error ? t("bench.lightFailed") : t("bench.light", { s: lighter.voice_secs.toFixed(1), wer: percent(lighter.listen_wer), big: percent(last.listen_wer), free: ((lighter.vram_free_mb ?? 0) / 1024).toFixed(1) }) }}</li>
        <li v-if="lighter && last.advice.asr_model">{{ last.advice.asr_model === last.asr_model ? t("bench.lightStays") : t("bench.lightChosen") }}</li>
      </ul>
      <p class="hint">{{ t("bench.measured", { when, time: duration(last.secs), version: last.vcp_version ?? "?" }) }}</p>
    </template>
    <p v-else-if="!running" class="hint">{{ t("bench.none") }}</p>

    <div v-if="restartSlow != null" class="mt-4 border-t border-line pt-4">
      <UiToggle v-model="restartModel" :label="t('bench.restart')" :tip="t('bench.restartTip')" :disabled="store.busy === 'benchSettings'" />
    </div>
  </section>
</template>
