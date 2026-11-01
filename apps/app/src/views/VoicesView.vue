<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Play, Spinner, Stop, Waveform } from "@ttv-tts/ui/icons";
import { api, EFFECTS, LANGUAGES, type SayReport, type Voice } from "../api";
import { host, run, store } from "../store";

const { t } = useI18n();
const voices = ref<Voice[]>([]);
const filter = ref("");
const text = ref("");
const voice = ref("");
const language = ref("German");
const emotion = ref("");
const fx = ref("");
const speaking = ref(false);
const report = ref<SayReport | null>(null);

const EMOTIONS = ["joy", "surprise", "awe", "sad", "fear", "anger", "disgust", "contempt"];
const ready = computed(() => !!host.value?.connected && host.value.kind !== "none");

async function load() {
  if (!ready.value) return;
  const v = await run("voices", () => api.voices());
  if (v) {
    voices.value = v;
    if (!voice.value && v.length) voice.value = v[0]!.id;
  }
}
onMounted(load);
watch(ready, (r) => r && load());

const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  return f ? voices.value.filter((v) => v.name.toLowerCase().includes(f)) : voices.value;
});

// Voices named "TTS-xx …" carry their language; pick it automatically.
watch(voice, (id) => {
  const m = voices.value.find((v) => v.id === id)?.name.match(/^TTS-([a-z]{2}) /);
  const lang = m && LANGUAGES.find(([, code]) => code === m[1]);
  if (lang) language.value = lang[0];
});

async function speak() {
  speaking.value = true;
  report.value = null;
  try {
    const r = await run("speak", () =>
      api.speakTest({ text: text.value.trim() || t("voices.textPlaceholder"), voice: voice.value, language: language.value, emotion: emotion.value || null, fx: fx.value || null }),
    );
    if (r) report.value = r;
  } finally {
    speaking.value = false;
  }
}

const reportText = computed(() => {
  const r = report.value;
  if (!r) return "";
  return t("voices.report", { first: `${(r.first_audio_ms / 1000).toFixed(1)} s`, audio: `${r.audio_secs.toFixed(1)} s`, rtf: r.rtf.toFixed(2) });
});
</script>

<template>
  <div class="space-y-5">
    <p v-if="!ready" class="notice notice-warn text-sm">{{ t("voices.noHost") }}</p>

    <section class="card p-5">
      <div class="card-head"><Play />{{ t("voices.tryTitle") }}</div>
      <p class="muted mb-3 text-sm">{{ t("voices.tryText") }}</p>
      <textarea v-model="text" rows="2" class="input h-auto py-2" :placeholder="t('voices.textPlaceholder')" />
      <div class="mt-3 grid gap-3 sm:grid-cols-[2fr_1fr_1fr_1fr]">
        <div>
          <label class="label">{{ t("voices.voice") }}</label>
          <select v-model="voice" class="input" :disabled="!voices.length">
            <option v-for="v in voices" :key="v.id" :value="v.id">{{ v.name }}</option>
          </select>
        </div>
        <div>
          <label class="label">{{ t("voices.language") }}</label>
          <select v-model="language" class="input">
            <option v-for="[name] in LANGUAGES" :key="name" :value="name">{{ name }}</option>
          </select>
        </div>
        <div>
          <label class="label">{{ t("voices.emotion") }}</label>
          <select v-model="emotion" class="input">
            <option value="">{{ t("voices.emotionNone") }}</option>
            <option v-for="e in EMOTIONS" :key="e" :value="e">{{ t(`emotions.${e}`) }}</option>
          </select>
        </div>
        <div>
          <label class="label">{{ t("voices.effect") }}<UiTip :text="t('tips.voiceEffects')" /></label>
          <select v-model="fx" class="input" data-fx>
            <option value="">{{ t("fx.none") }}</option>
            <option v-for="e in EFFECTS" :key="e" :value="e">{{ t(`fx.names.${e}`) }}</option>
          </select>
        </div>
      </div>
      <div class="mt-4 flex flex-wrap items-center gap-3">
        <button class="btn btn-primary" :disabled="!ready || !voice || speaking" @click="speak">
          <Spinner v-if="speaking" :size="16" class="spin" /><Play v-else :size="16" />{{ speaking ? t("voices.speaking") : t("voices.speak") }}
        </button>
        <button v-if="speaking" class="btn" @click="api.skip()"><Stop :size="16" />{{ t("common.stop") }}</button>
        <span v-if="report" class="text-sm text-muted">{{ reportText }}<UiTip :text="t('tips.rtf')" /></span>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head">
        <Waveform />{{ t("voices.title") }}<span class="badge ml-1">{{ t("voices.count", voices.length) }}</span>
        <input v-model="filter" class="input ml-auto max-w-56" :placeholder="t('voices.filter')" />
      </div>
      <p v-if="store.busy === 'voices'" class="flex items-center gap-2 text-sm text-muted"><Spinner :size="15" class="spin" />{{ t("common.loading") }}</p>
      <ul v-else class="grid max-h-[28rem] gap-1 overflow-y-auto sm:grid-cols-2">
        <li v-for="v in shown" :key="v.id">
          <button type="button" class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-sm hover:bg-panel-2" :class="voice === v.id && 'bg-accent-soft'" @click="voice = v.id">
            <span class="min-w-0 flex-1 truncate">{{ v.name }}</span>
            <span class="text-xs text-muted">{{ v.duration_seconds.toFixed(1) }} s</span>
            <span class="dot" :class="v.has_voice_clone_prompt ? 'dot-ok' : ''" :data-tip="`${v.has_voice_clone_prompt ? t('voices.primed') : t('voices.notPrimed')}\n${t('tips.primed')}`" />
          </button>
        </li>
      </ul>
    </section>
  </div>
</template>
