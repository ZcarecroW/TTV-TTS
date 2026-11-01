<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { duration } from "@ttv-tts/ui/format";
import { Bolt, ClockRotate, Gauge, iconByName, Plus, Sparkle, Spinner } from "@ttv-tts/ui/icons";
import { presets, type ClassicFound, type PresetView } from "../api";
import ComputeDialog from "../components/ComputeDialog.vue";
import JobList from "../components/JobList.vue";
import PresetDetailDialog from "../components/PresetDetailDialog.vue";
import PresetEditorDialog from "../components/PresetEditorDialog.vue";
import { activeJobs, host, loadEngine, loadPresets, run, store, toast } from "../store";

const { t } = useI18n();
const detail = ref<string | null>(null);
const computeFor = ref<string | null>(null);
const editor = ref<{ id?: string } | null>(null);
const classic = ref<ClassicFound | null>(null);

const connected = computed(() => !!host.value?.connected);

async function refresh() {
  await loadPresets();
  if (!connected.value) return;
  await loadEngine();
  // Voices from TTV-TTS 1.x that aren't adopted yet.
  if (!store.presets.some((p) => p.id === "classic")) classic.value = await presets.detectClassic().catch(() => null);
}
onMounted(refresh);
watch(connected, (c) => c && refresh());

async function adoptClassic() {
  const r = await run("classic", () => presets.adoptClassic());
  if (r) {
    toast("ok", t("presets.classicDone", { roles: r.roles, n: r.adopted }));
    classic.value = null;
    await loadPresets();
  }
}

async function measure() {
  const r = await run("benchmark", () => presets.benchmark());
  if (r != null) {
    toast("ok", t("compute.measured", { s: r.toFixed(1) }));
    await loadEngine();
  }
}

const builtin = computed(() => store.presets.filter((p) => p.builtin));
const own = computed(() => store.presets.filter((p) => !p.builtin));
const langs = (p: PresetView) => Object.entries(p.computed).sort((a, b) => b[1] - a[1]);
const pct = (p: PresetView) => (p.count ? Math.round((p.ready / p.count) * 100) : 0);

function openCompute(id: string) {
  detail.value = null;
  computeFor.value = id;
}
function openEditor(id?: string) {
  detail.value = null;
  editor.value = { id };
}
</script>

<template>
  <div class="space-y-5">
    <p v-if="!connected" class="notice notice-warn text-sm">{{ t("presets.noHost") }}</p>

    <div v-if="classic && classic.voices" class="notice notice-info text-sm">
      <ClockRotate :size="18" />
      <span class="flex-1">{{ t("presets.classicFound", { n: classic.voices, roles: classic.roles, langs: classic.languages.length }) }}<UiTip :text="t('tips.classic')" /></span>
      <button class="btn btn-primary btn-sm" :disabled="store.busy === 'classic'" @click="adoptClassic">
        <Spinner v-if="store.busy === 'classic'" :size="14" class="spin" />{{ t("presets.classicAdopt") }}
      </button>
    </div>

    <section v-if="store.engine?.jobs.length" class="card p-5">
      <div class="card-head"><Sparkle />{{ t("jobs.title") }}<span v-if="activeJobs.length" class="badge badge-accent ml-1">{{ activeJobs.length }}</span></div>
      <p class="muted mb-3 text-sm">{{ host?.kind === "direct" ? t("jobs.hintDirect") : t("jobs.hintProxy") }}</p>
      <JobList />
    </section>

    <div class="flex flex-wrap items-center gap-3">
      <span class="flex items-center gap-1.5 text-sm text-muted">
        <Gauge :size="16" />
        <template v-if="store.engine?.secs_per_voice">{{ t("presets.speed", { s: store.engine.secs_per_voice.toFixed(1), h: duration(store.engine.secs_per_voice * 240) }) }}</template>
        <template v-else>{{ t("presets.speedUnknown") }}</template>
        <UiTip :text="t('tips.speed')" />
      </span>
      <button v-if="connected && !store.engine?.secs_per_voice" class="btn btn-sm" :disabled="store.busy === 'benchmark'" @click="measure">
        <Spinner v-if="store.busy === 'benchmark'" :size="14" class="spin" /><Bolt v-else :size="14" />{{ store.busy === "benchmark" ? t("compute.measuring") : t("compute.measure") }}
      </button>
      <span v-if="store.library" class="text-sm text-muted">· {{ t("presets.librarySize", store.library.size) }}</span>
      <button class="btn btn-primary ml-auto" @click="openEditor()"><Plus :size="16" />{{ t("presets.new") }}</button>
    </div>

    <template v-for="group in [{ key: 'own', list: own }, { key: 'builtin', list: builtin }]" :key="group.key">
      <section v-if="group.list.length">
        <h2 class="mb-2.5 text-sm font-semibold text-muted">{{ t(`presets.${group.key}`) }}</h2>
        <div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          <article v-for="p in group.list" :key="p.id" class="card flex flex-col p-4">
            <div class="flex items-start gap-3">
              <span class="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent-text"><component :is="iconByName(p.icon)" :size="22" /></span>
              <div class="min-w-0 flex-1">
                <h3 class="truncate font-semibold">{{ p.name }}</h3>
                <p class="line-clamp-2 text-sm text-muted" :data-tip="p.description.length > 70 ? p.description : undefined">{{ p.description }}</p>
              </div>
            </div>
            <div class="mt-3 flex flex-wrap gap-1">
              <span v-for="[l, n] in langs(p)" :key="l" class="badge" :class="n >= p.count ? 'badge-ok' : 'badge-accent'">{{ l }} {{ n }}</span>
              <span v-if="!p.ready" class="text-xs text-muted">{{ t("presets.notYet") }}</span>
            </div>
            <div class="mt-3 flex items-center gap-2 text-xs text-muted">
              <div class="bar bar-thin flex-1"><i :style="{ width: `${pct(p)}%` }" /></div>
              <span>{{ t("presets.ready", { n: p.ready, total: p.count }) }}</span>
            </div>
            <div class="mt-3.5 flex gap-2">
              <button class="btn btn-sm flex-1" @click="detail = p.id">{{ t("presets.details") }}</button>
              <button class="btn btn-sm btn-primary flex-1" :disabled="!connected" @click="openCompute(p.id)"><Sparkle :size="14" />{{ t("presets.compute") }}</button>
            </div>
          </article>
        </div>
      </section>
    </template>

    <PresetDetailDialog v-if="detail" :preset-id="detail" @close="detail = null" @compute="openCompute" @edit="openEditor" />
    <ComputeDialog v-if="computeFor" :preset-id="computeFor" @close="computeFor = null" />
    <PresetEditorDialog v-if="editor" :preset-id="editor.id" @close="editor = null" @saved="(id) => ((editor = null), (detail = id))" />
  </div>
</template>
