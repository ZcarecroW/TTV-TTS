<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { CircleInfo, Gear, Key, Link, Play, Refresh, Stop, Waveform } from "@ttv-tts/ui/icons";
import { api } from "../api";
import { config, refresh, run, save, status, store, toast } from "../store";

const { t } = useI18n();
const v = computed(() => status.value!.vcp);

const keyInput = ref("");
const url = ref(config.value!.vcp_url);
const exe = ref(config.value!.vcp_exe ?? "");

function action(a: "start" | "stop" | "restart") {
  if (a === "stop" && !confirm(t("vcp.stopConfirm"))) return;
  void run("vcp", async () => toast("ok", await api.vcpAction(a)));
}

const keyState = computed(() => {
  const s = v.value;
  if (!store.ov!.has_vcp_key) {
    if (s.key_required === false) return { text: t("vcp.keyNotNeeded"), cls: "badge" };
    return { text: t("vcp.keyMissing"), cls: s.key_required ? "badge-err" : "badge" };
  }
  if (s.key_ok === true) return { text: t("vcp.keyVerified"), cls: "badge-ok" };
  if (s.key_ok === false) return { text: t("vcp.keyRejected"), cls: "badge-err" };
  return { text: t("vcp.keyStored"), cls: "badge-accent" };
});

async function saveKey() {
  const r = await run("key", () => api.setVcpKey(keyInput.value));
  if (r) {
    keyInput.value = "";
    toast(r.verified ? "ok" : "info", r.message);
    await refresh();
  }
}

async function detectKey() {
  const r = await run("key", () => api.detectVcpKey());
  if (r) {
    toast("ok", r.message);
    await refresh();
  }
}

async function createKey() {
  const k = await run("key", () => api.createVcpKey());
  if (k) {
    toast("ok", t("vcp.keyCreated"));
    await refresh();
  }
}

async function clearKey() {
  const ov = await run("key", () => api.clearVcpKey());
  if (ov) store.ov = ov;
}

const startWith = computed({ get: () => config.value!.start_vcp, set: (x: boolean) => void save({ start_vcp: x }) });
const keep = computed({ get: () => config.value!.keep_vcp_running, set: (x: boolean) => void save({ keep_vcp_running: x }) });
const hang = computed({ get: () => config.value!.restart_on_hang, set: (x: boolean) => void save({ restart_on_hang: x }) });
const hangSecs = computed({ get: () => config.value!.hang_restart_secs, set: (x: number) => void save({ hang_restart_secs: x }) });

function saveUrl() {
  if (url.value.trim() !== config.value!.vcp_url) void save({ vcp_url: url.value.trim() });
}
function saveExe() {
  if (exe.value.trim() !== (config.value!.vcp_exe ?? "")) void save({ vcp_exe: exe.value.trim() });
}
</script>

<template>
  <div class="space-y-5">
    <section class="card p-5">
      <div class="card-head"><Waveform />{{ t("vcp.statusTitle") }}</div>
      <dl class="grid gap-x-6 gap-y-2 text-sm sm:grid-cols-[auto_1fr]">
        <dt class="muted">{{ t("vcp.api") }}</dt>
        <dd class="flex items-center gap-2">
          <span class="dot" :class="v.reachable ? 'dot-ok' : 'dot-err'" />
          <span>{{ v.reachable ? t("vcp.apiOk", { url: v.url }) : t("vcp.apiOff", { url: v.url }) }}</span>
        </dd>
        <dt class="muted">{{ t("overview.version") }}</dt>
        <dd>{{ v.version ?? "–" }}</dd>
        <dt class="muted">{{ t("overview.model") }}</dt>
        <dd>{{ v.model_loaded ? [v.active_model, v.model_name].filter(Boolean).join(" · ") : t("overview.modelNone") }}</dd>
        <dt class="muted">{{ t("overview.hardware") }}</dt>
        <dd class="whitespace-pre-line">{{ v.hardware ?? "–" }}</dd>
        <dt class="muted">{{ t("vcp.program") }}</dt>
        <dd class="truncate font-mono text-xs leading-5">{{ v.exe ?? "–" }}<span v-if="v.pid" class="text-muted"> · {{ t("vcp.pid") }} {{ v.pid }}</span></dd>
      </dl>
      <p v-if="v.error && !v.reachable" class="notice notice-err mt-3"><CircleInfo :size="18" /><span>{{ v.error }}</span></p>
      <div class="mt-4 flex flex-wrap gap-2">
        <button class="btn btn-primary btn-sm" :disabled="v.reachable || v.starting || store.busy === 'vcp'" @click="action('start')"><Play :size="15" />{{ t("vcp.start") }}</button>
        <button class="btn btn-sm" :disabled="!v.running || store.busy === 'vcp'" @click="action('restart')"><Refresh :size="15" />{{ t("vcp.restart") }}</button>
        <button class="btn btn-danger btn-sm" :disabled="!v.running || store.busy === 'vcp'" @click="action('stop')"><Stop :size="15" />{{ t("vcp.stop") }}</button>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><Key />{{ t("vcp.keyTitle") }}<span class="badge ml-auto" :class="keyState.cls">{{ keyState.text }}</span></div>
      <p class="muted mb-3 text-sm">{{ t("vcp.keyText") }}</p>
      <div class="flex max-w-xl gap-2">
        <input v-model="keyInput" type="password" autocomplete="off" class="input mono" :placeholder="t('vcp.keyPlaceholder')" @keydown.enter="saveKey" />
        <button class="btn btn-primary" :disabled="!keyInput.trim() || store.busy === 'key'" @click="saveKey">{{ t("vcp.keySave") }}</button>
        <button v-if="v.reachable && v.key_ok !== true" class="btn" :disabled="store.busy === 'key'" :data-tip="t('tips.keyDetect')" @click="detectKey">{{ t("vcp.keyDetect") }}</button>
      </div>
      <div v-if="v.reachable && v.key_required === false && !store.ov!.has_vcp_key" class="notice notice-warn mt-3">
        <Key :size="18" />
        <div class="flex-1">
          <p>{{ t("vcp.keyCreateText") }}</p>
          <button class="btn btn-sm mt-2" :disabled="store.busy === 'key'" @click="createKey">{{ t("vcp.keyCreate") }}</button>
        </div>
      </div>
      <button v-if="store.ov!.has_vcp_key" class="btn btn-ghost btn-sm mt-3" @click="clearKey">{{ t("vcp.keyClear") }}</button>
    </section>

    <div class="grid gap-5 md:grid-cols-2">
      <section class="card p-5">
        <div class="card-head"><Link />{{ t("vcp.urlTitle") }}<UiTip :text="t('tips.vcpUrl')" /></div>
        <p class="muted mb-3 text-sm">{{ t("vcp.urlText") }}</p>
        <input v-model="url" class="input mono" @blur="saveUrl" @keydown.enter="saveUrl" />
      </section>

      <section class="card p-5">
        <div class="card-head"><Gear />{{ t("vcp.autoTitle") }}</div>
        <div class="space-y-4">
          <div>
            <label class="label" for="exe">{{ t("vcp.exe") }}<UiTip :text="t('tips.exe')" /></label>
            <input id="exe" v-model="exe" class="input mono" :placeholder="t('vcp.exePlaceholder')" @blur="saveExe" @keydown.enter="saveExe" />
          </div>
          <UiToggle v-model="startWith" :label="t('vcp.startWith')" :tip="t('tips.startWith')" />
          <UiToggle v-model="keep" :label="t('vcp.keepRunning')" :tip="t('tips.keepRunning')" />
          <div class="flex flex-wrap items-center gap-3">
            <UiToggle v-model="hang" :label="t('vcp.restartHang')" :tip="t('tips.restartHang')" />
            <label v-if="hang" class="flex items-center gap-2 text-sm text-muted">
              {{ t("vcp.hangAfter") }}
              <select v-model.number="hangSecs" class="input w-28">
                <option v-for="s in [90, 180, 300, 600]" :key="s" :value="s">{{ s < 120 ? t("common.seconds", { n: s }) : t("common.minutes", { n: s / 60 }) }}</option>
              </select>
            </label>
          </div>
        </div>
      </section>
    </div>

    <section class="card p-5">
      <div class="card-head"><CircleInfo />{{ t("vcp.helpTitle") }}</div>
      <p class="whitespace-pre-line text-sm">{{ t("vcp.helpSteps") }}</p>
    </section>
  </div>
</template>
