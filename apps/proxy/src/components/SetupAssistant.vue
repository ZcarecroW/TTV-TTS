<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { CircleCheck, Key, ShieldCheck, Spinner, Sparkle } from "@ttv-tts/ui/icons";
import logo from "../assets/logo.svg";
import { api, errText } from "../api";
import { checkFirewall, config, openPairing, refresh, run, save, status, store, toast } from "../store";

const { t } = useI18n();
const step = ref(0);
const total = 5;
const keyInput = ref("");
const v = computed(() => status.value!.vcp);
const vcpOk = computed(() => v.value.reachable && v.value.key_ok !== false);
const fw = computed(() => store.firewall);
const fwOk = computed(() => !fw.value?.managed || (fw.value.blocks === 0 && (fw.value.rule_ok || fw.value.allows > 0)));

async function saveKey() {
  const r = await run("key", () => api.setVcpKey(keyInput.value));
  if (r) {
    keyInput.value = "";
    await refresh();
  }
}
async function createKey() {
  if (await run("key", () => api.createVcpKey())) await refresh();
}
async function chooseMode(mode: "vcp" | "own") {
  if (mode === "own" && config.value!.key_mode !== "own") await run("key", () => api.newAccessKey());
  else await save({ key_mode: mode }, true);
  await refresh();
}
async function allow() {
  try {
    store.firewall = await api.firewallFix(false);
  } catch (e) {
    const m = errText(e);
    if (m !== "cancelled") toast("err", m);
  }
}
async function finish(pair: boolean) {
  await save({ setup_done: true }, true);
  if (pair) await openPairing();
}
function next() {
  step.value++;
  if (step.value === 3) void checkFirewall();
}
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-40 flex items-center justify-center bg-bg/95 p-6">
      <div class="card w-full max-w-xl p-7">
        <div class="mb-5 flex items-center gap-3">
          <img :src="logo" alt="" width="40" height="40" class="rounded-lg" />
          <div>
            <h2 class="text-lg font-semibold">{{ t("setup.title") }}</h2>
            <p class="text-xs text-muted">{{ t("setup.step", { n: step + 1, total }) }}</p>
          </div>
          <div class="ml-auto flex gap-1.5">
            <span v-for="i in total" :key="i" class="h-1.5 w-6 rounded-full" :class="i - 1 <= step ? 'bg-accent' : 'bg-line-strong'" />
          </div>
        </div>

        <!-- 1 welcome -->
        <div v-if="step === 0" class="space-y-3">
          <h3 class="text-base font-semibold">{{ t("setup.welcomeTitle") }}</h3>
          <p class="text-sm">{{ t("setup.welcomeText") }}</p>
          <p class="muted text-sm">{{ t("setup.welcomeList") }}</p>
        </div>

        <!-- 2 VCP -->
        <div v-else-if="step === 1" class="space-y-3">
          <h3 class="text-base font-semibold">{{ t("setup.vcpTitle") }}</h3>
          <p v-if="v.reachable" class="notice notice-ok"><CircleCheck :size="18" />{{ t("setup.vcpOk", { version: v.version ?? "" }) }}</p>
          <div v-else class="notice notice-warn">
            <Spinner :size="18" class="spin" />
            <div><p class="font-medium">{{ t("setup.vcpWait") }}</p><p class="text-xs">{{ t("setup.vcpWaitText") }}</p></div>
          </div>
          <template v-if="v.reachable">
            <div v-if="v.key_ok === false">
              <label class="label">{{ t("setup.vcpKeyNeeded") }}</label>
              <div class="flex gap-2">
                <input v-model="keyInput" type="password" autocomplete="off" class="input mono" :placeholder="t('vcp.keyPlaceholder')" @keydown.enter="saveKey" />
                <button class="btn btn-primary" :disabled="!keyInput.trim() || store.busy === 'key'" @click="saveKey">{{ t("vcp.keySave") }}</button>
              </div>
            </div>
            <p v-else-if="store.ov!.has_vcp_key" class="notice notice-ok"><Key :size="18" />{{ t("setup.vcpKeyOk") }}</p>
            <div v-else-if="v.key_required === false" class="notice notice-info">
              <Key :size="18" />
              <div class="flex-1"><p class="text-sm">{{ t("setup.vcpNoKey") }}</p><button class="btn btn-sm mt-2" @click="createKey">{{ t("vcp.keyCreate") }}</button></div>
            </div>
          </template>
        </div>

        <!-- 3 access -->
        <div v-else-if="step === 2" class="space-y-3">
          <h3 class="text-base font-semibold">{{ t("setup.accessTitle") }}</h3>
          <p class="text-sm">{{ t("setup.accessText") }}</p>
          <div class="grid gap-2 sm:grid-cols-2">
            <button
              v-for="m in (['vcp', 'own'] as const)"
              :key="m"
              type="button"
              class="rounded-xl border p-3.5 text-left transition-colors"
              :class="config!.key_mode === m ? 'border-accent bg-accent-soft' : 'border-line hover:bg-panel-2'"
              :disabled="m === 'vcp' && !store.ov!.has_vcp_key"
              @click="chooseMode(m)"
            >
              <div class="text-sm font-semibold">{{ m === "vcp" ? t("setup.accessVcp") : t("setup.accessOwn") }}</div>
              <div class="mt-0.5 text-xs text-muted">{{ m === "vcp" ? t("setup.accessVcpText") : t("setup.accessOwnText") }}</div>
            </button>
          </div>
        </div>

        <!-- 4 firewall -->
        <div v-else-if="step === 3" class="space-y-3">
          <h3 class="text-base font-semibold">{{ t("setup.firewallTitle") }}</h3>
          <p class="text-sm">{{ t("setup.firewallText") }}</p>
          <p v-if="store.firewallChecking" class="flex items-center gap-2 text-sm text-muted"><Spinner :size="15" class="spin" />{{ t("common.checking") }}</p>
          <p v-else-if="fwOk" class="notice notice-ok"><CircleCheck :size="18" />{{ t("firewall.allowed") }}</p>
          <button v-else class="btn btn-primary" @click="allow"><ShieldCheck :size="16" />{{ t("firewall.allowPrivate") }}</button>
        </div>

        <!-- 5 done -->
        <div v-else class="space-y-3">
          <h3 class="flex items-center gap-2 text-base font-semibold"><Sparkle class="text-accent-2" />{{ t("setup.doneTitle") }}</h3>
          <p class="text-sm">{{ t("setup.doneText") }}</p>
        </div>

        <div class="mt-7 flex items-center gap-2">
          <button v-if="step > 0" class="btn btn-ghost" @click="step--">{{ t("common.back") }}</button>
          <button v-if="step < total - 1" class="btn btn-ghost ml-auto" @click="finish(false)">{{ t("common.skip") }}</button>
          <button v-if="step < total - 1" class="btn btn-primary" :disabled="step === 1 && !vcpOk" @click="next">{{ t("common.next") }}</button>
          <template v-else>
            <button class="btn ml-auto" @click="finish(false)">{{ t("setup.finish") }}</button>
            <button class="btn btn-primary" @click="finish(true)">{{ t("setup.pairNow") }}</button>
          </template>
        </div>
      </div>
    </div>
  </Teleport>
</template>
