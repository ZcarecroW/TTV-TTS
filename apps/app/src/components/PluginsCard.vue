<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";
import { ChevronDown, FolderOpen, Layers, Mask, Pen, Plus, Refresh, Server, Spinner, Trash, Warning } from "@ttv-tts/ui/icons";
import { plugins, type PluginInfo, type PluginsView } from "../api";
import { run, store, toast } from "../store";

// Effect plugins (VST3, CLAP): a chain on every voice, and plugin effects
// viewers get at random. With TTV-TTS Proxy they run on the voice host PC
// when it has them too.
const { t } = useI18n();
const view = ref<PluginsView | null>(null);
const load = async () => (view.value = (await plugins.list().catch(() => null)) ?? view.value);
onMounted(async () => {
  await load();
  // the voice host PC's plugins (it scans them the first time it's asked)
  if (view.value) view.value = (await plugins.host().catch(() => null)) ?? view.value;
});

const host = computed(() => view.value?.host ?? null);
const hostHas = (id: string) => !!host.value?.installed && !host.value.missing.includes(id) && !host.value.errors[id];
/** Where a slot or effect runs, for its badge. */
function where(id: string): "host" | "here" | "hostOnly" | "broken" | null {
  const v = view.value;
  if (!v) return null;
  const bad = !!v.errors[id];
  if (host.value && hostHas(id) && (v.host_runs || bad)) return v.host_runs ? "host" : "hostOnly";
  if (bad) return "broken";
  return host.value?.installed ? "here" : null;
}

// the add dialog: for the chain on every voice, or as a viewer effect
const adding = ref<"main" | "variant" | null>(null);
const found = ref<PluginInfo[] | null>(null);
const query = ref("");
const variantName = ref("");
async function openAdd(kind: "main" | "variant") {
  adding.value = kind;
  variantName.value = "";
  if (found.value) return;
  found.value = (await run("plugins-scan", () => plugins.scan())) ?? [];
}
/** Local plugins, then the ones only the voice host PC has. */
const choices = computed(() => {
  const local = (found.value ?? []).map((p) => ({ p, host: false }));
  const extra = (host.value?.installed ?? []).filter((h) => !local.some((l) => l.p.format === h.format && l.p.id === h.id)).map((p) => ({ p, host: true }));
  const q = query.value.trim().toLowerCase();
  return [...local, ...extra].filter(({ p }) => !q || p.name.toLowerCase().includes(q) || p.vendor.toLowerCase().includes(q));
});

async function add(p: PluginInfo) {
  const kind = adding.value;
  const name = variantName.value.trim() || p.name;
  const v = await run("plugins-add", () => (kind === "variant" ? plugins.addVariant(p, name) : plugins.add(p)));
  if (v) {
    view.value = v;
    adding.value = null;
    toast("ok", kind === "variant" ? t("settings.variantAdded", { name }) : t("settings.pluginsAdded", { name: p.name }));
  }
}
async function fromFile() {
  const list = await run("plugins-pick", () => plugins.pick());
  if (list?.length === 1) await add(list[0]!);
  else if (list?.length) found.value = list;
}
const act = async (f: () => Promise<PluginsView>) => {
  const v = await run("plugins", f);
  if (v) view.value = v;
};
async function edit(id: string) {
  await run("plugins-edit", () => plugins.edit(id).then(() => true));
}
async function editOnHost(id: string) {
  const ok = await run("plugins-edit", () => plugins.editOnHost(id).then(() => true));
  if (ok) toast("info", t("settings.pluginsEditedOnHost"));
}
async function rescanHost() {
  const v = await run("plugins-host", () => plugins.hostScan());
  if (v) view.value = v;
  setTimeout(async () => (view.value = (await plugins.host().catch(() => null)) ?? view.value), 4000);
}
const canEditHere = (id: string) => !!view.value?.editors && !view.value.errors[id];
const fmt = (p: PluginInfo) => (p.format === "clap" ? "CLAP" : "VST3");
</script>

<template>
  <section class="card p-5 md:col-span-2" data-plugins>
    <div class="card-head">
      <Layers />{{ t("settings.pluginsTitle") }}<UiTip :text="t('tips.plugins')" />
      <UiDocs class="ml-auto" page="effects" anchor="plugins" />
    </div>

    <!-- the voice host PC -->
    <div v-if="host" class="mb-4 flex flex-wrap items-center gap-2 rounded-xl border border-line p-3 text-sm" data-plugins-host>
      <Server :size="16" class="shrink-0 text-muted" />
      <span v-if="host.scanning" class="flex items-center gap-1.5 text-muted"><Spinner :size="14" class="spin" />{{ t("settings.pluginsHostScanning") }}</span>
      <span v-else-if="host.installed">{{ view?.host_runs ? t("settings.pluginsHostRuns", { n: host.installed.length }) : t("settings.pluginsHostOff", { n: host.installed.length }) }}</span>
      <span v-else class="text-muted">{{ t("settings.pluginsHostUnknown") }}</span>
      <button class="btn btn-ghost btn-sm ml-auto" :disabled="host.scanning || store.busy === 'plugins-host'" @click="rescanHost"><Refresh :size="14" />{{ t("settings.pluginsHostScan") }}</button>
    </div>

    <!-- on every voice -->
    <div class="mb-1.5 flex items-center gap-2">
      <span class="label m-0">{{ t("settings.pluginsMain") }}</span>
      <button class="btn btn-sm ml-auto" @click="openAdd('main')"><Plus :size="15" />{{ t("settings.pluginsAdd") }}</button>
    </div>
    <p v-if="!view?.slots.length" class="text-sm text-muted">{{ t("settings.pluginsEmpty") }}</p>
    <ol v-else class="space-y-1.5">
      <li v-for="(s, i) in view.slots" :key="s.id" class="flex flex-wrap items-center gap-2 rounded-lg bg-panel-2 px-3 py-2 text-sm" :class="!s.enabled && 'opacity-60'" data-plugin>
        <span class="w-5 text-right font-mono text-xs text-muted">{{ i + 1 }}</span>
        <span class="font-medium">{{ s.plugin.name }}</span>
        <span class="badge">{{ fmt(s.plugin) }}</span>
        <span class="text-xs text-muted">{{ s.plugin.vendor }}</span>
        <span v-if="where(s.id) === 'host'" class="badge badge-ok" :data-tip="t('settings.pluginsWhereHostTip')"><Server :size="12" />{{ t("settings.pluginsWhereHost") }}</span>
        <span v-else-if="where(s.id) === 'hostOnly'" class="badge badge-warn" :data-tip="t('settings.pluginsWhereHostOnlyTip')"><Server :size="12" />{{ t("settings.pluginsWhereHostOnly") }}</span>
        <span v-else-if="where(s.id) === 'here' && host" class="badge" :data-tip="t('settings.pluginsWhereHereTip')">{{ t("settings.pluginsWhereHere") }}</span>
        <span v-if="where(s.id) === 'broken'" class="badge badge-err" :data-tip="view.errors[s.id]"><Warning :size="12" />{{ t("settings.pluginsBroken") }}</span>
        <span class="ml-auto flex items-center gap-1">
          <UiToggle :model-value="s.enabled" :label="t('settings.pluginsOn')" @update:model-value="act(() => plugins.toggle(s.id, $event))" />
          <button v-if="canEditHere(s.id) || !hostHas(s.id)" class="btn btn-ghost btn-sm" :disabled="!view.editors || !s.enabled || !!view.errors[s.id]" :data-tip="view.editors ? t('settings.pluginsEditTip') : t('settings.pluginsNoEditor')" @click="edit(s.id)"><Pen :size="14" />{{ t("settings.pluginsEdit") }}</button>
          <button v-if="hostHas(s.id) && host?.editors" class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.pluginsEditOnHost')" :data-tip="t('settings.pluginsEditOnHost')" @click="editOnHost(s.id)"><Server :size="14" /></button>
          <button class="btn btn-ghost btn-sm btn-icon" :disabled="i === 0" :aria-label="t('settings.pluginsUp')" :data-tip="t('settings.pluginsUp')" @click="act(() => plugins.move(s.id, -1))"><ChevronDown :size="14" class="rotate-180" /></button>
          <button class="btn btn-ghost btn-sm btn-icon" :disabled="i === view.slots.length - 1" :aria-label="t('settings.pluginsDown')" :data-tip="t('settings.pluginsDown')" @click="act(() => plugins.move(s.id, 1))"><ChevronDown :size="14" /></button>
          <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.pluginsRemove')" :data-tip="t('settings.pluginsRemove')" @click="act(() => plugins.remove(s.id))"><Trash :size="14" /></button>
        </span>
      </li>
    </ol>

    <!-- plugin effects for viewers -->
    <div class="mt-5 mb-1.5 flex items-center gap-2" data-variants>
      <Mask :size="16" class="text-muted" />
      <span class="label m-0">{{ t("settings.variantsTitle") }}</span><UiTip :text="t('tips.viewerPlugins')" />
      <button class="btn btn-sm ml-auto" :disabled="(view?.variants.length ?? 0) >= 12" @click="openAdd('variant')"><Plus :size="15" />{{ t("settings.variantsAdd") }}</button>
    </div>
    <p class="mb-2 text-xs text-muted">{{ t("settings.variantsText") }} <UiDocs page="effects" anchor="viewer-plugins" /></p>
    <p v-if="!view?.variants.length" class="text-sm text-muted">{{ t("settings.variantsEmpty") }}</p>
    <ul v-else class="space-y-1.5">
      <li v-for="v in view.variants" :key="v.id" class="flex flex-wrap items-center gap-2 rounded-lg bg-panel-2 px-3 py-2 text-sm" :class="!v.enabled && 'opacity-60'" data-variant>
        <input class="input h-8 w-44 py-0 text-sm font-medium" :value="v.name" :aria-label="t('settings.variantName')" @change="act(() => plugins.setVariant(v.id, { name: ($event.target as HTMLInputElement).value }))" />
        <span class="text-xs text-muted">{{ v.plugin.name }} · {{ fmt(v.plugin) }}</span>
        <span v-if="where(v.id) === 'host'" class="badge badge-ok" :data-tip="t('settings.pluginsWhereHostTip')"><Server :size="12" />{{ t("settings.pluginsWhereHost") }}</span>
        <span v-else-if="where(v.id) === 'hostOnly'" class="badge badge-warn" :data-tip="t('settings.pluginsWhereHostOnlyTip')"><Server :size="12" />{{ t("settings.pluginsWhereHostOnly") }}</span>
        <span v-else-if="where(v.id) === 'broken'" class="badge badge-err" :data-tip="view.errors[v.id]"><Warning :size="12" />{{ t("settings.pluginsBroken") }}</span>
        <span class="ml-auto flex items-center gap-1">
          <select class="input h-8 w-20 py-0 text-sm" :value="v.weight" :aria-label="t('settings.variantWeight')" :data-tip="t('settings.variantWeightTip')" @change="act(() => plugins.setVariant(v.id, { weight: Number(($event.target as HTMLSelectElement).value) }))">
            <option v-for="w in 5" :key="w" :value="w">×{{ w }}</option>
          </select>
          <UiToggle :model-value="v.enabled" :label="t('settings.pluginsOn')" @update:model-value="act(() => plugins.setVariant(v.id, { enabled: $event }))" />
          <button v-if="canEditHere(v.id) || !hostHas(v.id)" class="btn btn-ghost btn-sm" :disabled="!view.editors || !v.enabled || !!view.errors[v.id]" :data-tip="view.editors ? t('settings.pluginsEditTip') : t('settings.pluginsNoEditor')" @click="edit(v.id)"><Pen :size="14" />{{ t("settings.pluginsEdit") }}</button>
          <button v-if="hostHas(v.id) && host?.editors" class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.pluginsEditOnHost')" :data-tip="t('settings.pluginsEditOnHost')" @click="editOnHost(v.id)"><Server :size="14" /></button>
          <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.pluginsRemove')" :data-tip="t('settings.pluginsRemove')" @click="act(() => plugins.removeVariant(v.id))"><Trash :size="14" /></button>
        </span>
      </li>
    </ul>
    <p class="mt-3 text-xs text-muted">{{ t("settings.pluginsFree") }} <UiDocs page="effects" anchor="free-plugins" :label="t('settings.pluginsFreeLink')" /></p>

    <UiModal v-if="adding" :title="adding === 'variant' ? t('settings.variantsAddTitle') : t('settings.pluginsAddTitle')" width="max-w-xl" @close="adding = null">
      <div class="space-y-3" data-plugin-list>
        <input v-if="adding === 'variant'" v-model="variantName" class="input" :placeholder="t('settings.variantNamePlaceholder')" />
        <div class="flex gap-2">
          <input v-model="query" class="input" :placeholder="t('settings.pluginsSearch')" />
          <button class="btn" @click="fromFile"><FolderOpen :size="15" />{{ t("settings.pluginsFromFile") }}</button>
        </div>
        <p v-if="store.busy === 'plugins-scan'" class="flex items-center gap-2 text-sm text-muted"><Spinner :size="15" class="spin" />{{ t("settings.pluginsScanning") }}</p>
        <p v-else-if="!choices.length" class="text-sm text-muted">{{ t("settings.pluginsNone") }}</p>
        <ul v-else class="max-h-80 space-y-1 overflow-y-auto">
          <li v-for="c in choices" :key="c.p.path + c.p.id">
            <button type="button" class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-panel-2" :disabled="store.busy === 'plugins-add'" @click="add(c.p)">
              <span class="font-medium">{{ c.p.name }}</span>
              <span class="badge">{{ fmt(c.p) }}</span>
              <span v-if="c.host" class="badge" :data-tip="t('settings.pluginsOnlyHostTip')"><Server :size="12" />{{ t("settings.pluginsOnlyHost") }}</span>
              <span class="ml-auto truncate text-xs text-muted">{{ c.p.vendor }}</span>
            </button>
          </li>
        </ul>
        <p class="hint">{{ t("settings.pluginsHint") }}</p>
      </div>
    </UiModal>
  </section>
</template>
