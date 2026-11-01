<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { duration } from "@ttv-tts/ui/format";
import { Pause, Play, Spinner, Stop, X } from "@ttv-tts/ui/icons";
import { presets, type JobView } from "../api";
import { loadEngine, run, store } from "../store";

const { t } = useI18n();

// Running and waiting jobs first, then the last finished ones.
const jobs = computed(() => {
  const all = store.engine?.jobs ?? [];
  const open = all.filter((j) => !["done", "cancelled", "failed"].includes(j.state));
  const closed = all.filter((j) => ["done", "cancelled", "failed"].includes(j.state)).sort((a, b) => (b.finished ?? 0) - (a.finished ?? 0));
  return [...open, ...closed.slice(0, 3)];
});

// Voices are designed, checked and saved per batch, so "done" grows in
// jumps; the bar also counts the progress inside the running batch.
function partial(j: JobView) {
  if (!j.batch || !j.steps || !["running", "waiting"].includes(j.state)) return 0;
  const p = Math.min(1, j.step / j.steps);
  const design = j.options.speech_check ? 0.75 : 0.85;
  if (j.phase === "design") return j.batch * design * p;
  if (j.phase === "check") return j.batch * (0.75 + 0.1 * p);
  return Math.max(0, j.batch - j.step) * 0.85; // saved voices already count as done
}
const pct = (j: JobView) => (j.total ? Math.min(100, Math.round(((j.done + j.failed + partial(j)) / j.total) * 100)) : 100);
const badge: Record<string, string> = { running: "badge-accent", waiting: "badge-warn", paused: "badge-warn", queued: "", done: "badge-ok", cancelled: "", failed: "badge-err" };

async function act(j: JobView, action: "pause" | "resume" | "cancel" | "remove") {
  await run(`job-${j.id}`, () => presets.job(j.id, action));
  await loadEngine();
}
</script>

<template>
  <ul v-if="jobs.length" class="space-y-3">
    <li v-for="j in jobs" :key="j.id" class="rounded-xl border border-line bg-panel-2/40 p-3.5">
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-medium">{{ j.preset_name }}</span>
        <span v-if="j.kind && j.kind !== 'compute'" class="badge">{{ t(`jobs.kind.${j.kind}`) }}</span>
        <span class="text-xs text-muted">{{ j.languages.join(" · ") }}</span>
        <span class="badge" :class="badge[j.state]">
          <Spinner v-if="j.state === 'running'" :size="12" class="spin" />{{ t(`jobs.state.${j.state}`) }}
        </span>
        <span class="ml-auto flex gap-1">
          <button v-if="j.state === 'running' || j.state === 'waiting' || j.state === 'queued'" class="btn btn-ghost btn-sm" :disabled="store.busy === `job-${j.id}`" @click="act(j, 'pause')"><Pause :size="15" />{{ t("jobs.pause") }}</button>
          <button v-if="j.state === 'paused'" class="btn btn-ghost btn-sm" :disabled="store.busy === `job-${j.id}`" @click="act(j, 'resume')"><Play :size="15" />{{ t("jobs.resume") }}</button>
          <button v-if="!['done', 'cancelled', 'failed'].includes(j.state)" class="btn btn-ghost btn-sm" :disabled="store.busy === `job-${j.id}`" @click="act(j, 'cancel')"><Stop :size="15" />{{ t("jobs.cancel") }}</button>
          <button v-else class="btn btn-ghost btn-sm btn-icon" :aria-label="t('jobs.remove')" :data-tip="t('jobs.remove')" @click="act(j, 'remove')"><X :size="15" /></button>
        </span>
      </div>
      <div class="bar mt-2.5"><i :style="{ width: `${pct(j)}%` }" /></div>
      <div class="mt-1.5 flex flex-wrap gap-x-4 gap-y-0.5 text-xs text-muted">
        <span>{{ t("jobs.progress", { done: j.done, total: j.total }) }}<template v-if="j.failed"> · {{ t("jobs.failed", { n: j.failed }) }}</template><template v-if="j.flagged"> · <span class="text-warn">{{ t("jobs.flagged", { n: j.flagged }) }}</span></template><template v-if="j.improved"> · {{ t("jobs.improved", { n: j.improved }) }}</template></span>
        <span v-if="j.state === 'running' && j.current" :data-tip="t('jobs.batchTip', { n: j.batch })">{{ t(`jobs.phase.${j.phase || "design"}`) }}<template v-if="j.steps"> {{ Math.min(j.step + 1, j.steps) }}/{{ j.steps }}</template>: {{ j.current }}</span>
        <span v-if="j.state === 'waiting'">{{ t("jobs.waitingText") }}</span>
        <span v-if="j.eta_secs != null && !['done', 'cancelled', 'failed'].includes(j.state)" class="ml-auto">{{ t("jobs.eta", { time: duration(j.eta_secs) }) }}</span>
        <span v-else-if="j.secs_per_voice" class="ml-auto">{{ t("jobs.speed", { s: j.secs_per_voice.toFixed(1) }) }}</span>
      </div>
      <p v-if="j.error" class="mt-1.5 text-xs text-err">{{ j.error }}</p>
    </li>
  </ul>
</template>
