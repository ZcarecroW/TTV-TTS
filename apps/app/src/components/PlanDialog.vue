<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import { duration } from "@ttv-tts/ui/format";
import { Play, Spinner } from "@ttv-tts/ui/icons";
import { presets as api } from "../api";
import { loadEngine, run, store } from "../store";

const emit = defineEmits<{ close: [] }>();
const { t, locale } = useI18n();

// Most speakers first: the plan works through them in this order.
const ALL = ["de", "en", "zh", "es", "fr", "pt", "ru", "ja", "ko", "it"];
const langs = ref([...ALL]);
const toggle = (l: string) => (langs.value = langs.value.includes(l) ? langs.value.filter((x) => x !== l) : ALL.filter((x) => x === l || langs.value.includes(x)));

const voices = computed(() => store.presets.filter((p) => p.builtin).reduce((n, p) => n + p.count, 0) * langs.value.length);
const secs = computed(() => voices.value * (store.engine?.secs_per_voice ?? 30) * 1.1);

async function start() {
  if ((await run("planStart", () => api.planStart(langs.value))) !== undefined) {
    await loadEngine();
    emit("close");
  }
}
</script>

<template>
  <UiModal docs-page="presets" docs-anchor="plan" :title="t('plan.startTitle')" width="max-w-xl" @close="emit('close')">
    <div class="space-y-4 text-sm">
      <p class="muted">{{ t("plan.startHint") }}</p>
      <div>
        <div class="label">{{ t("compute.languages") }}</div>
        <div class="flex flex-wrap gap-1.5">
          <button v-for="l in ALL" :key="l" type="button" class="chip" :class="langs.includes(l) && 'chip-on'" :aria-pressed="langs.includes(l)" @click="toggle(l)">{{ t(`lang.${l}`) }}</button>
        </div>
      </div>
      <p>{{ t("plan.startSize", { voices: voices.toLocaleString(locale), time: duration(secs) }) }}</p>
    </div>
    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.cancel") }}</button>
      <button class="btn btn-primary" :disabled="!langs.length || store.busy === 'planStart'" @click="start">
        <Spinner v-if="store.busy === 'planStart'" :size="15" class="spin" /><Play v-else :size="15" />{{ t("plan.startButton") }}
      </button>
    </template>
  </UiModal>
</template>
