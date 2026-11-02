<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiCopy from "@ttv-tts/ui/components/UiCopy.vue";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Layers, Pen, Refresh, Spinner, Trash, Tv, Warning } from "@ttv-tts/ui/icons";
import { api, type ObsLinks, type PluginsHere } from "../api";
import { config, run, save } from "../store";

// The profiles this proxy reads for OBS: TTV-TTS hands them over ("OBS
// through the voice host PC"), the proxy reads their chat while a browser
// source listens and plays the voices there.
const { t } = useI18n();
const data = ref<ObsLinks | null>(null);
const port = ref(config.value!.obs_port);

const plugins = ref<PluginsHere | null>(null);
async function load() {
  data.value = await api.obsLinks().catch(() => null);
  plugins.value = await api.plugins().catch(() => null);
}
async function rescan() {
  plugins.value = (await run("plugins", () => api.pluginsRescan())) ?? plugins.value;
}
const openPlugin = (slot: string) => void run("plugins", () => api.pluginsOpen(slot).then(() => true));
let timer = 0;
onMounted(() => {
  void load();
  timer = window.setInterval(load, 3000);
});
onBeforeUnmount(() => clearInterval(timer));

async function remove(id: string, name: string) {
  if (!confirm(t("obs.removeConfirm", { name }))) return;
  const d = await run("obs", () => api.obsRemove(id));
  if (d) data.value = d;
}
const savePort = () => void save({ obs_port: Number(port.value) || 8112 }).then(load);
</script>

<template>
  <div class="space-y-5">
    <section class="card p-5">
      <div class="card-head"><Tv />{{ t("obs.title") }}<UiTip :text="t('obs.tip')" /><UiDocs class="ml-auto" page="obs" anchor="host" /></div>
      <p class="text-sm text-muted">{{ t("obs.intro") }}</p>
      <p v-if="data?.error" class="notice notice-err mt-3 text-sm">{{ data.error }}</p>
      <p v-if="data && !data.links.length" class="mt-4 rounded-xl bg-panel-2 p-4 text-sm text-muted">{{ t("obs.empty") }}</p>
      <div v-if="data?.links.length" class="mt-4 space-y-3">
        <div v-for="l in data.links" :key="l.id" class="rounded-xl border border-line bg-panel-2 p-4">
          <div class="flex flex-wrap items-center gap-2">
            <b>{{ l.name }}</b>
            <span class="text-xs text-muted">{{ l.channels.map((c) => `#${c}`).join(" ") }}</span>
            <span class="badge" :class="l.listeners ? 'badge-ok' : ''">{{ l.listeners ? t("obs.listening", { n: l.listeners }) : t("obs.notListening") }}</span>
            <span v-if="l.reading" class="badge badge-accent">{{ t("obs.reading") }}</span>
            <button class="btn btn-ghost btn-sm btn-icon ml-auto" :aria-label="t('obs.remove')" :data-tip="t('obs.remove')" @click="remove(l.id, l.name)"><Trash :size="15" /></button>
          </div>
          <p v-if="l.status.current" class="mt-2 truncate text-sm"><b>{{ l.status.current.display }}</b> {{ l.status.current.speech }}</p>
          <div class="mt-2 space-y-1">
            <div v-for="u in l.lan" :key="u" class="flex items-center gap-1">
              <input readonly class="input mono min-w-0 flex-1 text-xs" :value="u" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="u" />
            </div>
            <div v-if="l.internet" class="flex items-center gap-1">
              <input readonly class="input mono min-w-0 flex-1 text-xs" :value="l.internet" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="l.internet" /><span class="text-xs text-muted">{{ t("obs.internet") }}</span>
              <span class="badge" :class="l.internet_secure ? 'badge-ok' : 'badge-warn'">{{ l.internet_secure ? t("obs.httpsBadge") : t("obs.plainBadge") }}</span>
            </div>
            <p v-if="l.securing" class="text-xs text-muted">{{ t("obs.securing") }}</p>
            <p v-else-if="l.internet && !l.internet_secure" class="text-xs text-warn">{{ t("obs.plainHint") }}</p>
          </div>
        </div>
      </div>
    </section>

    <section v-if="plugins" class="card p-5" data-plugins-here>
      <div class="card-head"><Layers />{{ t("obs.pluginsTitle") }}<UiTip :text="t('obs.pluginsTip')" /><UiDocs class="ml-auto" page="effects" anchor="where" /></div>
      <div class="flex flex-wrap items-center gap-2 text-sm">
        <span v-if="plugins.scanning" class="flex items-center gap-1.5 text-muted"><Spinner :size="14" class="spin" />{{ t("obs.pluginsScanning") }}</span>
        <span v-else-if="plugins.installed">{{ t("obs.pluginsFound", { n: plugins.installed.length }) }}</span>
        <span v-else class="text-muted">{{ t("obs.pluginsNotScanned") }}</span>
        <button class="btn btn-ghost btn-sm ml-auto" :disabled="plugins.scanning" @click="rescan"><Refresh :size="14" />{{ t("obs.pluginsRescan") }}</button>
      </div>
      <div class="label mt-4">{{ t("obs.pluginsChains") }}</div>
      <p v-if="!plugins.chains.main.length && !plugins.chains.variants.length" class="text-sm text-muted">{{ t("obs.pluginsNone") }}</p>
      <ul v-else class="space-y-1 text-sm">
        <li v-for="s in [...plugins.chains.main.map((x) => ({ id: x.id, name: x.plugin.name, what: t('obs.pluginsEveryVoice'), on: x.enabled })), ...plugins.chains.variants.map((x) => ({ id: x.id, name: `${x.name} (${x.plugin.name})`, what: t('obs.pluginsViewer'), on: x.enabled }))]" :key="s.id" class="flex flex-wrap items-center gap-2 rounded-lg bg-panel-2 px-3 py-1.5" :class="!s.on && 'opacity-60'">
          <span class="font-medium">{{ s.name }}</span><span class="badge">{{ s.what }}</span>
          <span v-if="plugins.missing.includes(s.id)" class="badge badge-warn"><Warning :size="12" />{{ t("obs.pluginsMissing") }}</span>
          <span v-else-if="plugins.errors[s.id]" class="badge badge-err" :data-tip="plugins.errors[s.id]"><Warning :size="12" />!</span>
          <button v-else-if="plugins.editors" class="btn btn-ghost btn-sm ml-auto" :data-tip="t('obs.pluginsOpenTip')" @click="openPlugin(s.id)"><Pen :size="14" />{{ t("obs.pluginsOpen") }}</button>
        </li>
      </ul>
    </section>

    <section class="card p-5">
      <div class="card-head">{{ t("obs.portTitle") }}<UiTip :text="t('obs.portTip')" /></div>
      <UiField :label="t('obs.port')"><input v-model.number="port" type="number" min="1024" max="65535" class="input w-32" @change="savePort" /></UiField>
      <p class="mt-2 text-xs text-muted">{{ t("obs.portHint", { port }) }} <UiDocs page="obs" anchor="online" /></p>
    </section>
  </div>
</template>
