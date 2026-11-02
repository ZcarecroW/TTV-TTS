<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiCopy from "@ttv-tts/ui/components/UiCopy.vue";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import HttpsStatus from "@ttv-tts/ui/components/HttpsStatus.vue";
import { ago } from "@ttv-tts/ui/format";
import { Refresh, Tv } from "@ttv-tts/ui/icons";
import { hosted, obs, type HostedView, type ObsStatus, type OutputMode, type Profile } from "../api";
import { config, guest, host, run, store, toast } from "../store";

// Where a profile's voices play: this PC's sound device, an OBS browser
// source served by this app, or one served by the voice host PC (which then
// reads the chat itself).
const p = defineModel<Profile>({ required: true });
const { t, locale } = useI18n();

const proxyReady = computed(() => host.value?.kind === "proxy" && !guest.value);
const options = computed(() => [
  { value: "device", label: t("output.device") },
  { value: "obs", label: t("output.obs") },
  ...(proxyReady.value || p.value.output.mode === "host_obs" ? [{ value: "host_obs", label: t("output.hostObs") }] : []),
]);
const mode = computed({
  get: () => p.value.output.mode,
  set: (m: string) => (p.value.output.mode = m as OutputMode),
});

// what the servers say (refreshed while the card is open)
const local = ref<ObsStatus | null>(null);
const remote = ref<HostedView | null>(null);
const remoteError = ref("");
async function refresh() {
  if (p.value.output.mode === "obs") {
    local.value = await obs.status().catch(() => null);
  } else if (p.value.output.mode === "host_obs") {
    try {
      remote.value = (await hosted.list()).find((h) => h.id === p.value.id) ?? null;
      remoteError.value = "";
    } catch (e) {
      remoteError.value = /too old for OBS links/.test(String(e)) ? t("output.proxyTooOld") : String(e);
    }
  }
}
let timer = 0;
onMounted(() => {
  void refresh();
  timer = window.setInterval(refresh, 3000);
});
onBeforeUnmount(() => clearInterval(timer));
// the profile is saved 600 ms after a change; look again after that
watch(
  () => [p.value.id, p.value.output.mode, p.value.output.online, p.value.output.token],
  () => setTimeout(refresh, 1200),
);

const link = computed(() => local.value?.links.find((l) => l.profile === p.value.id) ?? null);
const listeners = computed(() => (p.value.output.mode === "obs" ? link.value?.listeners : remote.value?.listeners) ?? 0);

// this app's OBS server: other PCs, port, public address (all profiles)
const lan = ref(config.value?.obs.lan ?? false);
const port = ref(config.value?.obs.port ?? 8131);
const publicHost = ref(config.value?.obs.public_host ?? "");
const https = ref(config.value?.obs.https ?? true);
const duckToken = ref("");
const isDuck = computed(() => /\.duckdns\.org(:\d+)?$/i.test(publicHost.value.trim()));
async function saveServer() {
  const token = duckToken.value.trim() || undefined;
  const s = await run("obs", () => obs.set(lan.value, Number(port.value) || 8131, publicHost.value, https.value, token));
  if (s) {
    local.value = s;
    duckToken.value = "";
    if (store.app) store.app.config.obs = { lan: lan.value, port: Number(port.value) || 8131, public_host: publicHost.value, https: https.value };
  }
}

async function newLink() {
  if (!confirm(t("output.newLinkConfirm"))) return;
  const v = await run("obs", () => obs.newLink(p.value.id));
  if (v) {
    store.reader!.store = v.store;
    const fresh = v.store.profiles.find((x) => x.id === p.value.id);
    if (fresh) p.value.output.token = fresh.output.token;
    toast("ok", t("output.newLinkDone"));
    setTimeout(refresh, 300);
  }
}
</script>

<template>
  <section class="card p-5" data-output>
    <div class="card-head"><Tv />{{ t("output.title") }}<UiTip :text="t('tips.output')" /><UiDocs class="ml-auto" page="obs" /></div>
    <div class="space-y-4">
      <UiSegmented v-model="mode" :options="options" />
      <p class="text-sm text-muted">{{ t(`output.${p.output.mode === "host_obs" ? "hostObs" : p.output.mode}Text`) }}</p>
      <p v-if="!proxyReady && p.output.mode !== 'host_obs'" class="text-xs text-muted">{{ t("output.hostObsNeedsProxy") }}</p>

      <!-- OBS on this PC -->
      <template v-if="p.output.mode === 'obs'">
        <div v-if="link" class="space-y-2">
          <div class="flex items-center gap-2">
            <span class="label m-0">{{ t("output.linkHere") }}</span>
            <span class="badge" :class="listeners ? 'badge-ok' : ''">{{ listeners ? t("output.listening", { n: listeners }) : t("output.notListening") }}</span>
          </div>
          <div class="flex items-center gap-1"><input readonly class="input mono min-w-0 flex-1 text-xs" :value="link.local" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="link.local" /></div>
          <div v-for="l in link.lan" :key="l" class="flex items-center gap-1">
            <input readonly class="input mono min-w-0 flex-1 text-xs" :value="l" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="l" /><span class="text-xs text-muted">{{ t("output.otherPc") }}</span>
          </div>
          <div v-if="link.online" class="flex items-center gap-1">
            <input readonly class="input mono min-w-0 flex-1 text-xs" :value="link.online" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="link.online" /><span class="text-xs text-muted">{{ t("output.internet") }}</span>
            <span class="badge" :class="link.online_secure ? 'badge-ok' : 'badge-warn'">{{ link.online_secure ? t("output.httpsBadge") : t("output.plainBadge") }}</span>
          </div>
          <p v-if="link.securing" class="text-xs text-muted">{{ t("output.securing") }}</p>
          <p v-else-if="link.online && !link.online_secure" class="text-xs text-warn">{{ t("output.plainHint") }}</p>
        </div>
        <p v-else-if="local?.error" class="text-sm text-err">{{ local.error }}</p>
        <p v-else class="text-sm text-muted">{{ t("common.loading") }}</p>
        <div class="grid gap-4 md:grid-cols-3">
          <UiToggle v-model="lan" :label="t('output.lan')" :tip="t('tips.outputLan')" @update:model-value="saveServer" />
          <UiField :label="t('output.port')"><input v-model.number="port" type="number" min="1024" max="65535" class="input w-28" @change="saveServer" /></UiField>
          <UiToggle v-model="p.output.online" :label="t('output.online')" :tip="t('tips.outputOnline')" />
        </div>
        <div v-if="p.output.online" class="grid gap-4 md:grid-cols-2">
          <UiField :label="t('output.publicHost')" :tip="t('tips.outputPublicHost')">
            <input v-model="publicHost" class="input mono" placeholder="mystream.duckdns.org" @change="saveServer" />
          </UiField>
          <UiField v-if="isDuck" :label="t('output.duckToken')" :tip="t('tips.duckdns')">
            <input v-model="duckToken" type="password" autocomplete="off" class="input mono" :placeholder="local?.duckdns_token ? t('output.duckTokenStored') : '1a2b3c4d-…'" @change="saveServer" />
          </UiField>
        </div>
        <template v-if="p.output.online && isDuck">
          <p v-if="local && !local.duckdns_token" class="text-xs text-warn">{{ t("output.duckNeedsToken") }}</p>
          <p v-else-if="local?.duckdns_update" class="text-xs" :class="local.duckdns_update.error ? 'text-warn' : 'text-muted'">{{ local.duckdns_update.error ?? t("output.duckUpdated", { ago: ago(local.duckdns_update.at, locale), ip: local.duckdns_update.ip ?? "?" }) }}</p>
          <UiToggle v-model="https" :label="t('friends.httpsToggle')" :hint="t('friends.httpsHint')" @update:model-value="saveServer" />
          <HttpsStatus v-if="local" :status="local.https" :on="https" />
        </template>
        <p v-if="p.output.online" class="text-xs text-muted">{{ t("output.onlineHint", { port }) }} <UiDocs page="obs" anchor="online" /></p>
      </template>

      <!-- OBS through the voice host PC -->
      <template v-if="p.output.mode === 'host_obs'">
        <p v-if="remoteError" class="text-sm text-err">{{ remoteError }}</p>
        <div v-else-if="remote" class="space-y-2">
          <div class="flex items-center gap-2">
            <span class="label m-0">{{ t("output.linkHost") }}</span>
            <span class="badge" :class="listeners ? 'badge-ok' : ''">{{ listeners ? t("output.listening", { n: listeners }) : t("output.notListening") }}</span>
            <span v-if="remote.reading" class="badge badge-accent">{{ t("output.reading") }}</span>
          </div>
          <div v-for="l in remote.lan" :key="l" class="flex items-center gap-1"><input readonly class="input mono min-w-0 flex-1 text-xs" :value="l" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="l" /></div>
          <div v-if="remote.internet" class="flex items-center gap-1">
            <input readonly class="input mono min-w-0 flex-1 text-xs" :value="remote.internet" @focus="($event.target as HTMLInputElement).select()" /><UiCopy :value="remote.internet" /><span class="text-xs text-muted">{{ t("output.internet") }}</span>
            <span class="badge" :class="remote.internet_secure ? 'badge-ok' : 'badge-warn'">{{ remote.internet_secure ? t("output.httpsBadge") : t("output.plainBadge") }}</span>
          </div>
          <p v-if="remote.securing" class="text-xs text-muted">{{ t("output.securing") }}</p>
          <p v-else-if="remote.internet && !remote.internet_secure" class="text-xs text-warn">{{ t("output.plainHostHint") }}</p>
        </div>
        <p v-else class="text-sm text-muted">{{ t("output.handingOver") }}</p>
        <UiToggle v-model="p.output.online" :label="t('output.online')" :tip="t('tips.outputOnlineHost')" />
        <p v-if="p.output.online && remote && !remote.internet && !remote.securing" class="text-xs text-muted">{{ t("output.onlineHostHint") }} <UiDocs page="obs" anchor="online" /></p>
      </template>

      <div v-if="p.output.mode !== 'device'" class="flex flex-wrap items-center gap-3">
        <button class="btn btn-sm" @click="newLink"><Refresh :size="15" />{{ t("output.newLink") }}</button>
        <span class="text-xs text-muted">{{ t("output.obsHowTo") }}</span>
      </div>
    </div>
  </section>
</template>
