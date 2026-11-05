<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { duration } from "@ttv-tts/ui/format";
import { ChevronDown, ChevronRight, Pause, Spinner, Stop, Trash, X } from "@ttv-tts/ui/icons";
import { presets, type JobView } from "../api";
import { loadEngine, run, store } from "../store";
import JobRow from "./JobRow.vue";

const { t, locale } = useI18n();

const CLOSED = ["done", "cancelled", "failed"];
const all = computed(() => store.engine?.jobs ?? []);
const atWork = (j: JobView) => j.state === "running" || j.state === "waiting";

/** The engine's order (`rank` in crates/engine): imports, then checks and
 * improvements, then what you computed yourself, ready-made packs last. */
const rank = (j: JobView) => (j.kind === "import" ? 0 : j.kind !== "compute" ? 1 : j.options.archive ? 3 : 2);

/** What runs and waits, in working order. */
const open = computed(() =>
  all.value.filter((j) => !CLOSED.includes(j.state) && j.state !== "paused").sort((a, b) => Number(atWork(b)) - Number(atWork(a)) || rank(a) - rank(b)),
);
const paused = computed(() => all.value.filter((j) => j.state === "paused"));
const closed = computed(() => all.value.filter((j) => CLOSED.includes(j.state)).sort((a, b) => (b.finished ?? 0) - (a.finished ?? 0)));

// A queue can hold hundreds of jobs: only the one at work gets a card, the
// next few a line each, the rest a line per language.
const now = computed(() => open.value.slice(0, 1));
const next = computed(() => open.value.slice(1, 4));
const later = computed(() => open.value.slice(4));

const left = (j: JobView) => Math.max(0, j.total - j.done - j.failed);
/** About 1 voice in 10 is improved afterwards; `secs_per_voice` doesn't include that. */
const secs = (j: JobView) => {
  const per = store.engine?.secs_per_voice;
  return per && j.kind === "compute" ? left(j) * per * (j.options.improve ? 1.1 : 1) : (j.eta_secs ?? 0);
};
const num = (n: number) => n.toLocaleString(locale.value);

const voicesLeft = computed(() => open.value.reduce((n, j) => n + left(j), 0));
const secsLeft = computed(() => open.value.reduce((n, j) => n + secs(j), 0));
const voicesDone = computed(() => (store.engine?.library_size ?? 0) + (store.engine?.archived ?? 0));
const until = computed(() => new Date(Date.now() + secsLeft.value * 1000).toLocaleDateString(locale.value, { day: "numeric", month: "long" }));

const groups = computed(() => {
  const by = new Map<string, { key: string; label: string; jobs: JobView[]; voices: number; secs: number }>();
  for (const j of later.value) {
    const key = j.languages.join(",");
    let g = by.get(key);
    if (!g) by.set(key, (g = { key, label: j.languages.length === 1 ? t(`lang.${j.languages[0]}`) : j.languages.join(" · "), jobs: [], voices: 0, secs: 0 }));
    g.jobs.push(j);
    g.voices += left(j);
    g.secs += secs(j);
  }
  return [...by.values()];
});

// One section open at a time keeps the page short; short sections stay open.
const opened = ref("");
const toggle = (key: string) => (opened.value = opened.value === key ? "" : key);
const shown = (key: string, list: JobView[]) => opened.value === key || list.length <= 3;

async function removeAll(list: JobView[], question: string) {
  if (!confirm(question)) return;
  await run("jobs-remove", async () => {
    for (const j of [...list]) await presets.job(j.id, "remove");
  });
  await loadEngine();
}

// Voices are designed, checked and saved per batch, so "done" grows in
// jumps; the bar also counts the progress inside the running batch.
function partial(j: JobView) {
  if (!j.batch || !j.steps || !atWork(j)) return 0;
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
  <div v-if="all.length" class="space-y-3">
    <!-- the whole queue in one line -->
    <div v-if="open.length > 1 && !store.engine?.plan" class="rounded-xl bg-panel-2/40 px-3.5 py-2.5">
      <div class="flex flex-wrap items-baseline gap-x-4 gap-y-0.5">
        <span class="font-medium">{{ t("jobs.overview", { voices: num(voicesLeft), jobs: num(open.length) }) }}</span>
        <span v-if="secsLeft" class="ml-auto text-sm">{{ t("jobs.overviewEta", { time: duration(secsLeft), date: until }) }}</span>
      </div>
      <p v-if="voicesDone" class="mt-0.5 text-xs text-muted">{{ t("jobs.overviewDone", { voices: num(voicesDone) }) }}</p>
    </div>

    <!-- the job at work -->
    <ul v-if="now.length" class="space-y-3">
      <li v-for="j in now" :key="j.id" class="rounded-xl border border-line bg-panel-2/40 p-3.5">
        <div class="flex flex-wrap items-center gap-2">
          <span class="font-medium">{{ j.preset_name || t("jobs.allPresets") }}</span>
          <span v-if="j.kind && j.kind !== 'compute'" class="badge">{{ t(`jobs.kind.${j.kind}`) }}</span>
          <span class="text-xs text-muted">{{ j.languages.join(" · ") }}</span>
          <span class="badge" :class="badge[j.state]">
            <Spinner v-if="j.state === 'running'" :size="12" class="spin" />{{ t(`jobs.state.${j.state}`) }}
          </span>
          <span class="ml-auto flex gap-1">
            <button class="btn btn-ghost btn-sm" :disabled="store.busy === `job-${j.id}`" @click="act(j, 'pause')"><Pause :size="15" />{{ t("jobs.pause") }}</button>
            <button class="btn btn-ghost btn-sm" :disabled="store.busy === `job-${j.id}`" @click="act(j, 'cancel')"><Stop :size="15" />{{ t("jobs.cancel") }}</button>
          </span>
        </div>
        <div class="bar mt-2.5"><i :style="{ width: `${pct(j)}%` }" /></div>
        <div class="mt-1.5 flex flex-wrap gap-x-4 gap-y-0.5 text-xs text-muted">
          <span>{{ t("jobs.progress", { done: j.done, total: j.total }) }}<template v-if="j.failed"> · {{ t("jobs.failed", { n: j.failed }) }}</template><template v-if="j.flagged"> · <span class="text-warn">{{ t("jobs.flagged", { n: j.flagged }) }}</span></template><template v-if="j.improved"> · {{ t("jobs.improved", { n: j.improved }) }}</template></span>
          <span v-if="j.state === 'running' && j.current" :data-tip="t('jobs.batchTip', { n: j.batch })">{{ t(`jobs.phase.${j.phase || "design"}`) }}<template v-if="j.steps"> {{ Math.min(j.step + 1, j.steps) }}/{{ j.steps }}</template>: {{ j.current }}</span>
          <span v-if="j.state === 'waiting'" :data-tip="t('jobs.waitingTip')">{{ t("jobs.waitingText") }}</span>
          <span v-if="j.eta_secs != null" class="ml-auto">{{ t("jobs.eta", { time: duration(j.eta_secs) }) }}</span>
          <span v-else-if="j.secs_per_voice" class="ml-auto">{{ t("jobs.speed", { s: j.secs_per_voice.toFixed(1) }) }}</span>
        </div>
        <p v-if="j.error" class="mt-1.5 text-xs text-err">{{ j.error }}</p>
      </li>
    </ul>

    <!-- next up, one line each -->
    <section v-if="next.length">
      <h4 class="text-xs font-medium text-muted">{{ t("jobs.next") }}</h4>
      <ul class="divide-y divide-line/50">
        <JobRow v-for="j in next" :key="j.id" :job="j" />
      </ul>
    </section>

    <!-- everything after that, by language -->
    <section v-if="groups.length">
      <h4 class="text-xs font-medium text-muted">{{ t("jobs.later") }}</h4>
      <div v-for="g in groups" :key="g.key">
        <button class="flex w-full items-center gap-2 py-1 text-left text-sm" :aria-expanded="opened === g.key" @click="toggle(g.key)">
          <ChevronDown v-if="opened === g.key" :size="14" class="text-muted" /><ChevronRight v-else :size="14" class="text-muted" />
          <span class="font-medium">{{ g.label }}</span>
          <span class="text-xs text-muted">{{ t("jobs.group", { jobs: g.jobs.length, voices: num(g.voices) }) }}</span>
          <span v-if="g.secs" class="ml-auto text-xs text-muted">{{ duration(g.secs) }}</span>
        </button>
        <ul v-if="opened === g.key" class="ml-5 divide-y divide-line/50">
          <JobRow v-for="j in g.jobs" :key="j.id" :job="j" />
        </ul>
      </div>
    </section>

    <!-- stopped by hand -->
    <section v-if="paused.length">
      <div class="flex items-center gap-2">
        <button class="flex items-center gap-2 py-1 text-left text-sm" :aria-expanded="shown('paused', paused)" @click="toggle('paused')">
          <ChevronDown v-if="shown('paused', paused)" :size="14" class="text-muted" /><ChevronRight v-else :size="14" class="text-muted" />
          <span class="font-medium">{{ t("jobs.pausedTitle", { n: paused.length }) }}</span>
        </button>
        <button class="btn btn-ghost btn-sm ml-auto" :disabled="store.busy === 'jobs-remove'" @click="removeAll(paused, t('jobs.removePausedConfirm', { n: paused.length }))"><Trash :size="15" />{{ t("jobs.removeAll") }}</button>
      </div>
      <ul v-if="shown('paused', paused)" class="ml-5 divide-y divide-line/50">
        <JobRow v-for="j in paused" :key="j.id" :job="j" />
      </ul>
    </section>

    <!-- history -->
    <section v-if="closed.length">
      <div class="flex items-center gap-2">
        <button class="flex items-center gap-2 py-1 text-left text-sm" :aria-expanded="opened === 'closed'" @click="toggle('closed')">
          <ChevronDown v-if="opened === 'closed'" :size="14" class="text-muted" /><ChevronRight v-else :size="14" class="text-muted" />
          <span class="font-medium">{{ t("jobs.closedTitle", { n: closed.length }) }}</span>
        </button>
        <button class="btn btn-ghost btn-sm ml-auto" :disabled="store.busy === 'jobs-remove'" @click="removeAll(closed, t('jobs.removeClosedConfirm', { n: closed.length }))"><X :size="15" />{{ t("jobs.removeAll") }}</button>
      </div>
      <ul class="ml-5 divide-y divide-line/50">
        <JobRow v-for="j in closed.slice(0, opened === 'closed' ? 20 : 3)" :key="j.id" :job="j" />
      </ul>
    </section>
  </div>
</template>
