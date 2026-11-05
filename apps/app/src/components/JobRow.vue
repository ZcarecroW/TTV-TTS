<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { Pause, Play, Stop, X } from "@ttv-tts/ui/icons";
import { presets, type JobView } from "../api";
import { loadEngine, run, store } from "../store";

// One line of a long job list; the job at work gets the big card in JobList.
const props = defineProps<{ job: JobView }>();
const { t } = useI18n();

const closed = computed(() => ["done", "cancelled", "failed"].includes(props.job.state));
const busy = computed(() => store.busy === `job-${props.job.id}`);

async function act(action: "pause" | "resume" | "cancel" | "remove") {
  await run(`job-${props.job.id}`, () => presets.job(props.job.id, action));
  await loadEngine();
}
</script>

<template>
  <li class="flex flex-wrap items-center gap-x-2 gap-y-0.5 py-1 text-sm">
    <span class="font-medium">{{ job.preset_name || t("jobs.allPresets") }}</span>
    <span v-if="job.kind && job.kind !== 'compute'" class="badge">{{ t(`jobs.kind.${job.kind}`) }}</span>
    <span v-if="job.state === 'failed' || job.state === 'cancelled'" class="badge" :class="{ 'badge-err': job.state === 'failed' }">{{ t(`jobs.state.${job.state}`) }}</span>
    <span class="text-xs text-muted">{{ job.languages.join(" · ") }}</span>
    <span class="text-xs text-muted">
      {{ t("jobs.progress", { done: job.done, total: job.total }) }}<template v-if="job.failed"> · {{ t("jobs.failed", { n: job.failed }) }}</template><template v-if="job.flagged"> · <span class="text-warn">{{ t("jobs.flagged", { n: job.flagged }) }}</span></template><template v-if="job.improved"> · {{ t("jobs.improved", { n: job.improved }) }}</template>
    </span>
    <span class="ml-auto flex gap-1">
      <button v-if="job.state === 'queued'" class="btn btn-ghost btn-sm btn-icon" :disabled="busy" :aria-label="t('jobs.pause')" :data-tip="t('jobs.pause')" @click="act('pause')"><Pause :size="15" /></button>
      <button v-if="job.state === 'paused'" class="btn btn-ghost btn-sm btn-icon" :disabled="busy" :aria-label="t('jobs.resume')" :data-tip="t('jobs.resume')" @click="act('resume')"><Play :size="15" /></button>
      <button v-if="!closed" class="btn btn-ghost btn-sm btn-icon" :disabled="busy" :aria-label="t('jobs.cancel')" :data-tip="t('jobs.cancel')" @click="act('cancel')"><Stop :size="15" /></button>
      <button v-else class="btn btn-ghost btn-sm btn-icon" :disabled="busy" :aria-label="t('jobs.remove')" :data-tip="t('jobs.remove')" @click="act('remove')"><X :size="15" /></button>
    </span>
    <p v-if="job.error" class="w-full text-xs text-err">{{ job.error }}</p>
  </li>
</template>
