<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import { ChevronDown, FolderOpen, Layers, Pen, Plus, Spinner, Trash, Warning } from "@ttv-tts/ui/icons";
import { plugins, type PluginInfo, type PluginsView } from "../api";
import { run, store, toast } from "../store";

// Effect plugins (VST3, CLAP) on every voice, after the built-in effects.
const { t } = useI18n();
const view = ref<PluginsView | null>(null);
const load = async () => (view.value = (await plugins.list().catch(() => null)) ?? view.value);
onMounted(load);

const adding = ref(false);
const found = ref<PluginInfo[] | null>(null);
const query = ref("");
async function openAdd() {
  adding.value = true;
  if (found.value) return;
  found.value = (await run("plugins-scan", () => plugins.scan())) ?? [];
}
const shown = computed(() => {
  const q = query.value.trim().toLowerCase();
  return (found.value ?? []).filter((p) => !q || p.name.toLowerCase().includes(q) || p.vendor.toLowerCase().includes(q));
});

async function add(p: PluginInfo) {
  const v = await run("plugins-add", () => plugins.add(p));
  if (v) {
    view.value = v;
    adding.value = false;
    toast("ok", t("settings.pluginsAdded", { name: p.name }));
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
</script>

<template>
  <section class="card p-5 md:col-span-2" data-plugins>
    <div class="card-head">
      <Layers />{{ t("settings.pluginsTitle") }}<UiTip :text="t('tips.plugins')" />
      <button class="btn btn-sm ml-auto" @click="openAdd"><Plus :size="15" />{{ t("settings.pluginsAdd") }}</button>
    </div>
    <p v-if="!view?.slots.length" class="text-sm text-muted">{{ t("settings.pluginsEmpty") }}</p>
    <ol v-else class="space-y-1.5">
      <li v-for="(s, i) in view.slots" :key="s.id" class="flex flex-wrap items-center gap-2 rounded-lg bg-panel-2 px-3 py-2 text-sm" :class="!s.enabled && 'opacity-60'" data-plugin>
        <span class="w-5 text-right font-mono text-xs text-muted">{{ i + 1 }}</span>
        <span class="font-medium">{{ s.plugin.name }}</span>
        <span class="badge">{{ s.plugin.format === "clap" ? "CLAP" : "VST3" }}</span>
        <span class="text-xs text-muted">{{ s.plugin.vendor }}</span>
        <span v-if="view.errors[s.id]" class="badge badge-err" :data-tip="view.errors[s.id]"><Warning :size="12" />{{ t("settings.pluginsBroken") }}</span>
        <span class="ml-auto flex items-center gap-1">
          <UiToggle :model-value="s.enabled" :label="t('settings.pluginsOn')" @update:model-value="act(() => plugins.toggle(s.id, $event))" />
          <button class="btn btn-ghost btn-sm" :disabled="!view.editors || !s.enabled || !!view.errors[s.id]" :data-tip="view.editors ? t('settings.pluginsEditTip') : t('settings.pluginsNoEditor')" @click="edit(s.id)"><Pen :size="14" />{{ t("settings.pluginsEdit") }}</button>
          <button class="btn btn-ghost btn-sm btn-icon" :disabled="i === 0" :aria-label="t('settings.pluginsUp')" :data-tip="t('settings.pluginsUp')" @click="act(() => plugins.move(s.id, -1))"><ChevronDown :size="14" class="rotate-180" /></button>
          <button class="btn btn-ghost btn-sm btn-icon" :disabled="i === view.slots.length - 1" :aria-label="t('settings.pluginsDown')" :data-tip="t('settings.pluginsDown')" @click="act(() => plugins.move(s.id, 1))"><ChevronDown :size="14" /></button>
          <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.pluginsRemove')" :data-tip="t('settings.pluginsRemove')" @click="act(() => plugins.remove(s.id))"><Trash :size="14" /></button>
        </span>
      </li>
    </ol>

    <UiModal v-if="adding" :title="t('settings.pluginsAddTitle')" width="max-w-xl" @close="adding = false">
      <div class="space-y-3" data-plugin-list>
        <div class="flex gap-2">
          <input v-model="query" class="input" :placeholder="t('settings.pluginsSearch')" />
          <button class="btn" @click="fromFile"><FolderOpen :size="15" />{{ t("settings.pluginsFromFile") }}</button>
        </div>
        <p v-if="store.busy === 'plugins-scan'" class="flex items-center gap-2 text-sm text-muted"><Spinner :size="15" class="spin" />{{ t("settings.pluginsScanning") }}</p>
        <p v-else-if="!shown.length" class="text-sm text-muted">{{ t("settings.pluginsNone") }}</p>
        <ul v-else class="max-h-80 space-y-1 overflow-y-auto">
          <li v-for="p in shown" :key="p.path + p.id">
            <button type="button" class="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-panel-2" :disabled="store.busy === 'plugins-add'" @click="add(p)">
              <span class="font-medium">{{ p.name }}</span>
              <span class="badge">{{ p.format === "clap" ? "CLAP" : "VST3" }}</span>
              <span class="ml-auto truncate text-xs text-muted">{{ p.vendor }}</span>
            </button>
          </li>
        </ul>
        <p class="hint">{{ t("settings.pluginsHint") }}</p>
      </div>
    </UiModal>
  </section>
</template>
