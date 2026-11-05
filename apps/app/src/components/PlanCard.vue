<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { duration } from "@ttv-tts/ui/format";
import { Layers, Pause, Play, Spinner, Stop } from "@ttv-tts/ui/icons";
import { presets, type PlanCell, type PlanRow } from "../api";
import { loadEngine, run, store } from "../store";

const { t, locale } = useI18n();

const plan = computed(() => store.engine?.plan ?? null);
const pct = computed(() => (plan.value?.voices_total ? Math.min(100, Math.round((plan.value.voices_done / plan.value.voices_total) * 100)) : 0));
const num = (n: number) => n.toLocaleString(locale.value);
const until = computed(() => {
  const s = plan.value?.eta_secs;
  return s == null ? "" : new Date(Date.now() + s * 1000).toLocaleDateString(locale.value, { day: "numeric", month: "long" });
});
const finished = computed(() => (plan.value?.finished ? new Date(plan.value.finished * 1000).toLocaleDateString(locale.value, { day: "numeric", month: "long" }) : ""));

// Voices finish every few seconds: the grid follows at a calm pace.
let timer = 0;
onMounted(() => {
  timer = window.setInterval(() => {
    if (plan.value?.state === "running") void loadEngine();
  }, 20_000);
});
onBeforeUnmount(() => clearInterval(timer));

async function act(action: "pause" | "resume" | "stop") {
  if (action === "stop" && !confirm(t("plan.stopConfirm"))) return;
  await run("plan", () => presets.planControl(action));
  await loadEngine();
}

/** A cell's look: how far this preset is in this language. */
function shade(row: PlanRow, c: PlanCell) {
  const share = row.total ? c.done / row.total : 0;
  if (c.gave_up) return "cell-warn";
  if (share >= 1) return "cell-full";
  return share > 0 ? "cell-part" : "";
}
function tip(row: PlanRow, c: PlanCell, lang: string) {
  let s = t("plan.cell", { preset: row.name, lang: t(`lang.${lang}`), done: c.done, total: row.total });
  if (c.odd) s += ` · ${t("jobs.flagged", { n: c.odd })}`;
  if (c.gave_up) s += ` · ${t("plan.gaveUpCell", { n: c.gave_up })}`;
  return s;
}
</script>

<template>
  <section v-if="plan" class="card p-5">
    <div class="card-head">
      <Layers />{{ t("plan.title") }}
      <span class="badge ml-1" :class="plan.state === 'done' ? 'badge-ok' : plan.state === 'paused' ? 'badge-warn' : 'badge-accent'"><Spinner v-if="plan.state === 'running'" :size="12" class="spin" />{{ t(`plan.state.${plan.state}`) }}</span>
      <span class="ml-auto flex gap-1">
        <button v-if="plan.state === 'running'" class="btn btn-ghost btn-sm" :disabled="store.busy === 'plan'" @click="act('pause')"><Pause :size="15" />{{ t("jobs.pause") }}</button>
        <button v-if="plan.state === 'paused'" class="btn btn-ghost btn-sm" :disabled="store.busy === 'plan'" @click="act('resume')"><Play :size="15" />{{ t("jobs.resume") }}</button>
        <button v-if="plan.state !== 'done'" class="btn btn-ghost btn-sm" :disabled="store.busy === 'plan'" @click="act('stop')"><Stop :size="15" />{{ t("plan.stop") }}</button>
      </span>
    </div>
    <p class="muted text-sm">{{ plan.state === "done" ? t("plan.doneHint", { date: finished }) : t("plan.hint") }}</p>

    <div class="mt-3 flex flex-wrap items-baseline gap-x-4 gap-y-0.5">
      <span class="font-medium">{{ t("plan.progress", { done: num(plan.voices_done), total: num(plan.voices_total) }) }}</span>
      <span v-if="plan.eta_secs" class="ml-auto text-sm">{{ t("jobs.overviewEta", { time: duration(plan.eta_secs), date: until }) }}</span>
    </div>
    <div class="bar mt-1.5"><i :style="{ width: `${pct}%` }" /></div>
    <p v-if="plan.voices_odd || plan.gave_up" class="mt-1.5 text-xs text-muted">
      <span v-if="plan.voices_odd">{{ t("jobs.flagged", { n: num(plan.voices_odd) }) }}</span>
      <span v-if="plan.gave_up" class="text-warn"><template v-if="plan.voices_odd"> · </template>{{ t("plan.gaveUp", { n: plan.gave_up }) }}</span>
    </p>

    <div class="mt-4 overflow-x-auto">
      <table class="plan-grid text-xs">
        <thead>
          <tr>
            <th />
            <th v-for="l in plan.languages" :key="l" class="font-medium text-muted" :aria-label="t(`lang.${l}`)">{{ l }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in plan.rows" :key="r.id">
            <th class="whitespace-nowrap pr-3 text-left font-normal">{{ r.name }}</th>
            <td v-for="(c, i) in r.cells" :key="i"><span class="plan-cell" :class="shade(r, c)" :data-tip="tip(r, c, plan.languages[i])" :aria-label="tip(r, c, plan.languages[i])" /></td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.plan-grid {
  border-collapse: separate;
  border-spacing: 2px;
}
.plan-cell {
  display: block;
  width: 1.25rem;
  height: 0.8rem;
  border-radius: 2px;
  background: var(--color-line);
}
.cell-part {
  background: color-mix(in oklab, var(--color-accent) 45%, var(--color-line));
}
.cell-full {
  background: var(--color-accent);
}
.cell-warn {
  background: var(--color-warn);
}
</style>
