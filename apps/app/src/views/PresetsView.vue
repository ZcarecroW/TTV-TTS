<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { duration } from "@ttv-tts/ui/format";
import { Bolt, CircleCheck, ClockRotate, Download, Gauge, iconByName, Plus, Sparkle, Spinner, Warning } from "@ttv-tts/ui/icons";
import { api, packs, presets, type CatalogPack, type ClassicFound, type PresetView } from "../api";
import ComputeDialog from "../components/ComputeDialog.vue";
import JobList from "../components/JobList.vue";
import PresetDetailDialog from "../components/PresetDetailDialog.vue";
import PresetEditorDialog from "../components/PresetEditorDialog.vue";
import ReviewDialog from "../components/ReviewDialog.vue";
import ImportDialog from "../components/ImportDialog.vue";
import ExportDialog from "../components/ExportDialog.vue";
import DownloadDialog from "../components/DownloadDialog.vue";
import { activeJobs, host, loadEngine, loadPresets, run, store, toast } from "../store";

const { t } = useI18n();
const detail = ref<string | null>(null);
const computeFor = ref<string | null>(null);
const editor = ref<{ id?: string } | null>(null);
const classic = ref<ClassicFound | null>(null);
/** The quality check's list: all presets ("") or one. */
const review = ref<string | null>(null);
/** A pack file to import (picked). */
const importing = ref<string | null>(null);
const exporting = ref<{ id: string; keys: string[] } | null>(null);
/** Ready-made packs (GitHub); empty while unknown or offline. */
const catalog = ref<CatalogPack[]>([]);
const downloading = ref<string | null>(null);
packs.catalog().then((c) => (catalog.value = c.packs)).catch(() => {});
/** Ready-made voices this preset could still get. */
const downloadable = (p: PresetView) => catalog.value.some((c) => c.preset === p.id && (p.computed[c.lang] ?? 0) < c.to);
async function pickPack() {
  const p = await run("pick", () => packs.pick());
  if (p) importing.value = p;
}
const odd = computed(() => store.presets.reduce((n, p) => n + (p.odd ?? 0), 0));

const connected = computed(() => !!host.value?.connected);
// A proxy from before presets (2.0.0-alpha.4) answers 404 to the engine.
const tooOld = computed(() => host.value?.kind === "proxy" && /404|unknown ttv-tts proxy endpoint/i.test(store.engineError));
async function updateProxy() {
  const check = await run("proxyUpdate", () => api.proxyUpdate("check"));
  if (check?.available && (await run("proxyUpdate", () => api.proxyUpdate("install"))) !== undefined) toast("info", t("host.proxyUpdating"));
  else if (check) toast("info", t("host.proxyUpToDate"));
}

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

// Compares the library with VCP and repairs what it can.
async function verify() {
  const r = await run("verify", () => presets.verify());
  if (!r) return;
  toast("ok", r.removed || r.measured ? t("presets.verifiedFixed", { n: r.checked, removed: r.removed, measured: r.measured }) : t("presets.verified", { n: r.checked }));
  await loadPresets();
}

const librarySize = computed(() => store.library?.size ?? store.engine?.library_size ?? 0);
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
    <div v-else-if="tooOld" class="notice notice-warn text-sm">
      <Warning :size="18" />
      <span class="flex-1">{{ t("presets.proxyTooOld", { version: host?.proxy?.info.version ?? "?" }) }}</span>
      <button class="btn btn-primary btn-sm" :disabled="store.busy === 'proxyUpdate'" @click="updateProxy">{{ t("host.proxyInstall") }}</button>
    </div>

    <div v-if="classic && classic.voices" class="notice notice-info text-sm">
      <ClockRotate :size="18" />
      <span class="flex-1">{{ t("presets.classicFound", { n: classic.voices, roles: classic.roles, langs: classic.languages.length }) }}<UiTip :text="t('tips.classic')" /></span>
      <button class="btn btn-primary btn-sm" :disabled="store.busy === 'classic'" @click="adoptClassic">
        <Spinner v-if="store.busy === 'classic'" :size="14" class="spin" />{{ t("presets.classicAdopt") }}
      </button>
    </div>

    <div v-if="odd" class="notice notice-warn text-sm">
      <Warning :size="18" />
      <span class="flex-1">{{ t("quality.notice", odd) }}</span>
      <button class="btn btn-primary btn-sm" @click="review = ''">{{ t("quality.open") }}</button>
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
      <button v-if="connected && librarySize" class="btn btn-ghost btn-sm" :disabled="store.busy === 'verify'" @click="verify">
        <Spinner v-if="store.busy === 'verify'" :size="14" class="spin" /><CircleCheck v-else :size="14" />{{ store.busy === "verify" ? t("presets.verifying") : t("presets.verify") }}
      </button>
      <UiTip v-if="connected && librarySize" :text="t('tips.verify')" />
      <button class="btn ml-auto" :disabled="!connected" :data-tip="t('packs.importTip')" @click="pickPack"><Download :size="16" />{{ t("packs.import") }}</button>
      <button class="btn btn-primary" @click="openEditor()"><Plus :size="16" />{{ t("presets.new") }}</button>
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
              <button v-if="p.odd" type="button" class="badge badge-warn" @click="review = p.id"><Warning :size="12" />{{ t("quality.toCheck", { n: p.odd }) }}</button>
            </div>
            <div class="mt-3 flex items-center gap-2 text-xs text-muted">
              <div class="bar bar-thin flex-1"><i :style="{ width: `${pct(p)}%` }" /></div>
              <span>{{ t("presets.ready", { n: p.ready, total: p.count }) }}</span>
            </div>
            <div class="mt-3.5 flex gap-2">
              <button class="btn btn-sm flex-1" @click="detail = p.id">{{ t("presets.details") }}</button>
              <button v-if="downloadable(p)" class="btn btn-sm flex-1" :disabled="!connected" :data-tip="t('downloads.tip')" @click="downloading = p.id"><Download :size="14" />{{ t("downloads.button") }}</button>
              <button class="btn btn-sm btn-primary flex-1" :disabled="!connected" @click="openCompute(p.id)"><Sparkle :size="14" />{{ t("presets.compute") }}</button>
            </div>
          </article>
        </div>
      </section>
    </template>

    <PresetDetailDialog v-if="detail" :preset-id="detail" @close="detail = null" @compute="openCompute" @edit="openEditor" @export="(id, keys) => (exporting = { id, keys })" />
    <ExportDialog v-if="exporting" :preset-id="exporting.id" :keys="exporting.keys.length ? exporting.keys : undefined" @close="exporting = null" />
    <ComputeDialog v-if="computeFor" :preset-id="computeFor" @close="computeFor = null" />
    <DownloadDialog v-if="downloading" :preset-id="downloading" :catalog="catalog" @close="downloading = null" />
    <ImportDialog v-if="importing" :path="importing" @close="importing = null" />
    <ReviewDialog v-if="review !== null" :preset-id="review || null" @close="review = null" />
    <PresetEditorDialog v-if="editor" :preset-id="editor.id" @close="editor = null" @saved="(id) => ((editor = null), (detail = id))" />
  </div>
</template>
