<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiStat from "@ttv-tts/ui/components/UiStat.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Bolt, Link, Microchip, Play, Power, Ram, Refresh, Server, Temperature, Waveform } from "@ttv-tts/ui/icons";
import { bytes } from "@ttv-tts/ui/format";
import { api } from "../api";
import HostSetup from "../components/HostSetup.vue";
import { apply, config, host, run, store, toast } from "../store";

const { t, locale } = useI18n();
const changing = ref(false);
const h = computed(() => host.value!);
const cfg = computed(() => config.value!.host);
const vcp = computed(() => h.value.vcp);
const gpu = computed(() => store.hostSystem?.gpus[0]);
const showSetup = computed(() => changing.value || h.value.kind === "none");

onMounted(() => void api.watchHostSystem(true));
onBeforeUnmount(() => void api.watchHostSystem(false));

// First 16 hex digits in groups of four, like TTV-TTS Proxy shows them.
const certShort = computed(() => {
  const fp = cfg.value.kind === "proxy" ? cfg.value.fingerprint : "";
  return fp.slice(0, 16).toUpperCase().match(/.{4}/g)?.join("-") ?? "";
});

const vcpBadge = computed(() => {
  const v = vcp.value;
  if (!v) return { text: "–", cls: "" };
  if (v.starting) return { text: t("host.vcpStarting"), cls: "badge-warn" };
  return v.reachable ? { text: t("host.vcpRunning"), cls: "badge-ok" } : { text: t("host.vcpStopped"), cls: "badge-err" };
});

async function vcpAction(a: "start" | "restart") {
  const m = await run("vcp", () => api.proxyVcpAction(a));
  if (m) toast("ok", m);
}
async function wake() {
  if ((await run("wake", () => api.wakeHost())) !== undefined) toast("info", t("host.wakeSent"));
}
async function forget() {
  if (!confirm(t("host.forgetConfirm"))) return;
  await apply(await run("forget", () => api.forgetHost()));
}
</script>

<template>
  <div class="space-y-5">
    <template v-if="showSetup">
      <section v-if="h.kind === 'none'" class="card p-5">
        <div class="card-head"><Server />{{ t("host.none") }}</div>
        <p class="text-sm text-muted">{{ t("host.noneText") }}</p>
      </section>
      <HostSetup @done="changing = false" />
      <button v-if="changing" class="btn btn-ghost" @click="changing = false">{{ t("common.cancel") }}</button>
    </template>

    <template v-else>
      <div class="grid gap-5 md:grid-cols-2">
        <section class="card p-5">
          <div class="card-head">
            <Link />{{ t("host.statusTitle") }}
            <span class="badge ml-auto" :class="h.connected ? 'badge-ok' : 'badge-err'">{{ h.connected ? t("host.connected") : t("host.disconnected") }}</span>
          </div>
          <dl class="space-y-2 text-sm">
            <div class="flex justify-between gap-3"><dt class="muted">{{ h.kind === "proxy" ? t("host.proxy") : t("host.direct") }}</dt><dd class="font-medium">{{ cfg.kind === "proxy" ? cfg.name : "" }}</dd></div>
            <div class="flex justify-between gap-3"><dt class="muted">{{ t("host.address") }}</dt><dd class="font-mono text-xs">{{ cfg.kind === "proxy" ? cfg.address : cfg.kind === "direct" ? cfg.url : "" }}</dd></div>
            <div v-if="certShort" class="flex justify-between gap-3"><dt class="muted">{{ t("host.certificate") }}<UiTip :text="t('tips.certificate')" /></dt><dd class="font-mono text-xs">{{ certShort }}</dd></div>
          </dl>
          <p v-if="h.error && !h.connected" class="notice notice-err mt-3 text-sm">{{ h.error }}</p>
          <div class="mt-4 flex flex-wrap gap-2">
            <button v-if="h.kind === 'proxy' && !h.connected" class="btn btn-primary btn-sm" :data-tip="t('tips.wake')" @click="wake"><Power :size="15" />{{ t("host.wake") }}</button>
            <button class="btn btn-sm" @click="changing = true">{{ t("host.change") }}</button>
            <button class="btn btn-danger btn-sm" @click="forget">{{ t("host.forget") }}</button>
          </div>
        </section>

        <section class="card p-5">
          <div class="card-head"><Waveform />{{ t("host.vcpTitle") }}<span class="badge ml-auto" :class="vcpBadge.cls">{{ vcpBadge.text }}</span></div>
          <dl v-if="vcp" class="space-y-2 text-sm">
            <div class="flex justify-between gap-3"><dt class="muted">{{ t("host.version") }}</dt><dd class="font-medium">{{ vcp.version ?? "–" }}</dd></div>
            <div class="flex justify-between gap-3"><dt class="muted">{{ t("host.model") }}</dt><dd class="truncate font-medium">{{ vcp.model_loaded ? vcp.model_name : t("host.modelNone") }}</dd></div>
            <div class="flex justify-between gap-3"><dt class="muted">{{ t("host.queue") }}</dt><dd class="font-medium">{{ vcp.generating ? t("host.queueBusy", { n: vcp.queue_size }) : t("host.queueIdle") }}</dd></div>
            <div v-if="vcp.hardware" class="flex justify-between gap-3"><dt class="muted">{{ t("host.hardware") }}</dt><dd class="text-right text-xs">{{ vcp.hardware.split("\n")[0] }}</dd></div>
          </dl>
          <div v-if="h.kind === 'proxy' && h.connected" class="mt-4 flex gap-2">
            <button v-if="!vcp?.reachable" class="btn btn-primary btn-sm" :disabled="store.busy === 'vcp' || vcp?.starting" @click="vcpAction('start')"><Play :size="15" />{{ t("host.startVcp") }}</button>
            <button v-else class="btn btn-sm" :disabled="store.busy === 'vcp'" @click="vcpAction('restart')"><Refresh :size="15" />{{ t("host.restartVcp") }}</button>
          </div>
        </section>
      </div>

      <section v-if="h.kind === 'proxy' && h.connected" class="card p-5">
        <div class="card-head"><Microchip />{{ t("host.systemTitle") }}</div>
        <div v-if="store.hostSystem" class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
          <UiStat :label="t('host.cpu')" :value="`${Math.round(store.hostSystem.cpu_percent)} %`" :ratio="store.hostSystem.cpu_percent / 100"><template #icon><Microchip :size="14" /></template></UiStat>
          <UiStat :label="t('host.ram')" :value="bytes(store.hostSystem.mem_used, locale)" :sub="`/ ${bytes(store.hostSystem.mem_total, locale)}`" :ratio="store.hostSystem.mem_used / store.hostSystem.mem_total"><template #icon><Ram :size="14" /></template></UiStat>
          <template v-if="gpu">
            <UiStat :label="t('host.gpu')" :value="gpu.usage_percent != null ? `${Math.round(gpu.usage_percent)} %` : '–'" :sub="gpu.name" :ratio="gpu.usage_percent != null ? gpu.usage_percent / 100 : null"><template #icon><Waveform :size="14" /></template></UiStat>
            <UiStat :label="t('host.vram')" :value="bytes(gpu.vram_used, locale)" :sub="gpu.vram_total ? `/ ${bytes(gpu.vram_total, locale)}` : undefined" :ratio="gpu.vram_used != null && gpu.vram_total ? gpu.vram_used / gpu.vram_total : null"><template #icon><Ram :size="14" /></template></UiStat>
            <UiStat v-if="gpu.temperature_c != null" :label="t('host.temp')" :value="`${gpu.temperature_c} °C`" :ratio="gpu.temperature_c / 95"><template #icon><Temperature :size="14" /></template></UiStat>
            <UiStat v-if="gpu.power_w != null" :label="t('host.power')" :value="`${Math.round(gpu.power_w)} W`"><template #icon><Bolt :size="14" /></template></UiStat>
          </template>
        </div>
        <p v-else class="text-sm text-muted">{{ t("common.loading") }}</p>
        <div v-if="gpu?.processes.length" class="mt-4">
          <div class="label">{{ t("host.gpuProcesses") }}<UiTip :text="t('tips.gpuProcesses')" /></div>
          <ul class="flex flex-wrap gap-1.5">
            <li v-for="p in gpu.processes" :key="p.pid" class="badge" :class="/voice ?creator/i.test(p.name) ? 'badge-accent' : ''">
              {{ p.name }}<span v-if="p.usage != null && p.usage >= 1" class="font-normal opacity-80">· {{ Math.round(p.usage) }} %</span><span v-if="p.vram" class="font-normal opacity-80">· {{ bytes(p.vram, locale) }}</span>
            </li>
          </ul>
        </div>
        <p class="hint">{{ t("host.systemHint") }}</p>
      </section>
    </template>
  </div>
</template>
