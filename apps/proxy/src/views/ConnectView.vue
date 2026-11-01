<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiSecret from "@ttv-tts/ui/components/UiSecret.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiCopy from "@ttv-tts/ui/components/UiCopy.vue";
import { Devices, Key, Link, Lock, Network, Trash, Wifi } from "@ttv-tts/ui/icons";
import { ago } from "@ttv-tts/ui/format";
import { api, type KeyMode } from "../api";
import { config, openPairing, run, save, status, store, toast } from "../store";

const { t, locale } = useI18n();

// Full URLs for manual setup (built here: "//" inside template attributes trips vue-tsc).
const urls = computed(() => status.value!.addresses.map((a) => ({ ...a, url: `https://${a.ip}:${config.value!.port}` })));

const accessKey = ref<string | null>(null);
const custom = ref("");
const showCustom = ref(false);

async function loadKey() {
  accessKey.value = await api.accessKey();
}
onMounted(loadKey);
watch(() => [config.value?.key_mode, store.ov?.has_vcp_key], loadKey);

const keyMode = computed({
  get: () => config.value!.key_mode,
  set: (v: KeyMode) => void save({ key_mode: v }, true),
});

async function newKey() {
  if (!confirm(t("connect.keyNewConfirm"))) return;
  const k = await run("key", () => api.newAccessKey());
  if (k) accessKey.value = k;
}

async function setCustom() {
  const ok = await run("key", () => api.setAccessKey(custom.value));
  if (ok !== undefined) {
    custom.value = "";
    showCustom.value = false;
    await loadKey();
    toast("ok", t("connect.saved"));
  }
}

async function revoke(id: string) {
  await run("revoke", () => api.revokeDevice(id));
}

// Network options are edited locally and saved on blur / toggle.
const name = ref(config.value!.name);
const port = ref(config.value!.port);
const httpOn = ref(config.value!.http_port !== 0);
const httpPort = ref(config.value!.http_port || 8111);
const discovery = computed({ get: () => config.value!.discovery, set: (v: boolean) => void save({ discovery: v }) });

function saveName() {
  if (name.value !== config.value!.name) void save({ name: name.value });
}
function savePort() {
  if (port.value !== config.value!.port) void save({ port: port.value });
}
watch(httpOn, (on) => void save({ http_port: on ? httpPort.value : 0 }));
function saveHttpPort() {
  if (httpOn.value && httpPort.value !== config.value!.http_port) void save({ http_port: httpPort.value });
}
</script>

<template>
  <div class="space-y-5">
    <section class="card p-5">
      <div class="card-head"><Link />{{ t("connect.pairTitle") }}</div>
      <p class="text-sm">{{ t("connect.pairText") }}</p>
      <p class="muted mt-2 text-sm">{{ t("connect.pairSteps") }}</p>
      <button class="btn btn-primary mt-4" :disabled="store.busy === 'pair'" @click="openPairing()"><Link :size="16" />{{ t("connect.pairButton") }}</button>
    </section>

    <section class="card p-5">
      <div class="card-head"><Devices />{{ t("connect.devicesTitle") }}<span class="badge ml-auto">{{ status!.devices.length }}</span></div>
      <p v-if="!status!.devices.length" class="muted text-sm">{{ t("connect.devicesEmpty") }}</p>
      <ul v-else class="divide-y divide-line">
        <li v-for="d in status!.devices" :key="d.id" class="flex items-center gap-3 py-2.5">
          <div class="grid size-9 place-items-center rounded-lg bg-accent-soft text-accent-text"><Devices :size="18" /></div>
          <div class="min-w-0 flex-1">
            <div class="truncate font-medium">{{ d.name }}</div>
            <div class="truncate text-xs text-muted">
              {{ t("connect.pairedAt", { when: ago(d.paired_at, locale) }) }} ·
              {{ d.last_seen ? t("connect.seen", { when: ago(d.last_seen, locale), address: d.last_address ?? "?" }) : t("connect.neverSeen") }}
            </div>
          </div>
          <button class="btn btn-danger btn-sm" :data-tip="t('tips.revoke')" @click="revoke(d.id)"><Trash :size="15" />{{ t("connect.revoke") }}</button>
        </li>
      </ul>
    </section>

    <section class="card p-5">
      <div class="card-head"><Key />{{ t("connect.keyTitle") }}</div>
      <p class="muted mb-3 text-sm">{{ t("connect.keyText") }}</p>
      <UiSegmented
        v-model="keyMode"
        :tip="t('tips.keyMode')"
        :options="[
          { value: 'vcp', label: t('connect.keyModeVcp') },
          { value: 'own', label: t('connect.keyModeOwn') },
        ]"
      />
      <div class="mt-3 max-w-xl">
        <UiSecret :value="accessKey" :empty="t('connect.keyNone')" />
      </div>
      <div v-if="config!.key_mode === 'own'" class="mt-3 flex flex-wrap items-center gap-2">
        <button class="btn btn-sm" :disabled="store.busy === 'key'" @click="newKey">{{ t("connect.keyNew") }}</button>
        <button class="btn btn-sm" @click="showCustom = !showCustom">{{ t("connect.keyCustom") }}</button>
        <template v-if="showCustom">
          <input v-model="custom" class="input mono max-w-xs" :placeholder="t('connect.keyCustomPlaceholder')" @keydown.enter="setCustom" />
          <button class="btn btn-primary btn-sm" :disabled="custom.trim().length < 12" @click="setCustom">{{ t("common.save") }}</button>
        </template>
      </div>
    </section>

    <div class="grid gap-5 md:grid-cols-2">
      <section class="card p-5">
        <div class="card-head"><Lock />{{ t("connect.detailsTitle") }}</div>
        <p class="muted mb-3 text-sm">{{ t("connect.detailsText") }}</p>
        <div class="label">{{ t("connect.addresses") }}</div>
        <ul class="mb-3 space-y-1">
          <li v-for="a in urls" :key="a.ip" class="flex items-center gap-1 rounded-lg bg-panel-2 py-0.5 pl-2.5 pr-0.5">
            <code class="flex-1 truncate font-mono text-sm">{{ a.url }}</code>
            <span class="text-xs text-muted">{{ a.interface }}</span>
            <UiCopy :value="a.url" />
          </li>
        </ul>
        <div class="label">{{ t("connect.certificate") }}<UiTip :text="t('tips.fingerprint')" /></div>
        <div class="flex items-center gap-1 rounded-lg bg-panel-2 py-0.5 pl-2.5 pr-0.5">
          <code class="flex-1 truncate font-mono text-sm font-semibold">{{ store.ov!.fingerprint_short }}</code>
          <UiCopy :value="status!.info.fingerprint" />
        </div>
        <code class="mt-1 block break-all font-mono text-[0.7rem] text-muted">{{ status!.info.fingerprint }}</code>
      </section>

      <section class="card p-5">
        <div class="card-head"><Network />{{ t("connect.networkTitle") }}</div>
        <div class="space-y-4">
          <div>
            <label class="label" for="pname">{{ t("connect.name") }}</label>
            <input id="pname" v-model="name" class="input" :placeholder="t('connect.namePlaceholder', { host: status!.info.name })" @blur="saveName" @keydown.enter="saveName" />
          </div>
          <div>
            <label class="label" for="pport">{{ t("connect.portLabel") }}<UiTip :text="t('tips.port')" /></label>
            <input id="pport" v-model.number="port" type="number" min="1024" max="65535" class="input max-w-32" @blur="savePort" @keydown.enter="savePort" />
          </div>
          <UiToggle v-model="discovery" :tip="t('tips.discovery', { port: config!.port })"><Wifi :size="15" class="mr-1 inline" />{{ t("connect.discovery") }}</UiToggle>
          <div class="flex flex-wrap items-center gap-3">
            <UiToggle v-model="httpOn" :label="t('connect.httpPort')" :tip="t('tips.httpPort')" />
            <input v-if="httpOn" v-model.number="httpPort" type="number" min="1024" max="65535" class="input max-w-28" @blur="saveHttpPort" @keydown.enter="saveHttpPort" />
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
