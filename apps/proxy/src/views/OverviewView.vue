<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import UiCopy from "@ttv-tts/ui/components/UiCopy.vue";
import UiStat from "@ttv-tts/ui/components/UiStat.vue";
import { Bolt, CircleCheck, CircleInfo, CircleX, Devices, Link, Microchip, Network, Play, Ram, Refresh, Temperature, Warning, Waveform } from "@ttv-tts/ui/icons";
import { ago, bytes, duration } from "@ttv-tts/ui/format";
import { api } from "../api";
import { config, openPairing, problems, run, status, store } from "../store";

const { t, locale } = useI18n();

onMounted(() => void api.watchSystem(true));
onBeforeUnmount(() => void api.watchSystem(false));

const vcp = computed(() => status.value!.vcp);
const devices = computed(() => status.value!.devices);
const lastDevice = computed(() => [...devices.value].sort((a, b) => (b.last_seen ?? 0) - (a.last_seen ?? 0))[0]);
const gpu = computed(() => store.system?.gpus[0]);
const ready = computed(() => problems.value.every((p) => p.kind === "info") && vcp.value.reachable);

const vcpLabel = computed(() => {
  const v = vcp.value;
  if (v.starting) return { text: t("overview.starting"), cls: "badge-warn" };
  if (v.reachable) return { text: t("overview.running"), cls: "badge-ok" };
  if (v.running) return { text: t("overview.noApi"), cls: "badge-err" };
  return { text: t("overview.stopped"), cls: "badge-err" };
});

const modelText = computed(() => {
  const v = vcp.value;
  if (!v.model_loaded) return t("overview.modelNone");
  return v.model_name ?? v.active_model ?? "–";
});

function vcpAction(action: "start" | "restart") {
  void run("vcp", () => api.vcpAction(action));
}
</script>

<template>
  <div class="space-y-5">
    <!-- Overall status -->
    <section class="card p-5">
      <div class="flex items-start gap-4">
        <div class="grid size-11 shrink-0 place-items-center rounded-xl" :class="ready ? 'bg-ok-soft text-ok' : 'bg-warn-soft text-warn'">
          <CircleCheck v-if="ready" :size="24" />
          <Warning v-else :size="24" />
        </div>
        <div class="min-w-0 flex-1">
          <h2 class="text-lg font-semibold">{{ ready ? t("status.ready") : t("status.attention") }}</h2>
          <p class="muted text-sm">
            {{ ready ? t("status.readyHint", { version: vcp.version ?? "", devices: t("status.devices", devices.length) }) : t("status.attentionHint") }}
          </p>
          <ul v-if="problems.length" class="mt-3 space-y-2">
            <li v-for="(p, i) in problems" :key="i" class="notice" :class="p.kind === 'err' ? 'notice-err' : p.kind === 'warn' ? 'notice-warn' : 'notice-info'">
              <CircleX v-if="p.kind === 'err'" :size="18" />
              <Warning v-else-if="p.kind === 'warn'" :size="18" />
              <CircleInfo v-else :size="18" />
              <span class="flex-1">{{ p.text }}</span>
              <button v-if="p.action" class="btn btn-sm" @click="p.action.run()">{{ p.action.label }}</button>
            </li>
          </ul>
        </div>
      </div>
    </section>

    <div class="grid gap-5 md:grid-cols-3">
      <!-- VCP -->
      <section class="card p-5">
        <div class="card-head"><Waveform />{{ t("overview.vcpTitle") }}<span class="badge ml-auto" :class="vcpLabel.cls">{{ vcpLabel.text }}</span></div>
        <dl class="space-y-2 text-sm">
          <div class="flex justify-between gap-3"><dt class="muted">{{ t("overview.version") }}</dt><dd class="truncate font-medium">{{ vcp.version ?? "–" }}</dd></div>
          <div class="flex justify-between gap-3">
            <dt class="muted">{{ t("overview.model") }}</dt>
            <dd class="truncate text-right font-medium" :data-tip="vcp.model_loaded ? vcp.active_model ?? undefined : t('overview.modelHint')">{{ modelText }}</dd>
          </div>
          <div class="flex justify-between gap-3">
            <dt class="muted">{{ t("overview.queue") }}</dt>
            <dd class="font-medium">{{ vcp.generating ? t("overview.queueBusy", { n: vcp.queue_size }) : t("overview.queueIdle") }}</dd>
          </div>
        </dl>
        <div class="mt-4 flex gap-2">
          <button v-if="!vcp.reachable" class="btn btn-primary btn-sm" :disabled="store.busy === 'vcp' || vcp.starting" @click="vcpAction('start')"><Play :size="15" />{{ t("overview.start") }}</button>
          <button v-else class="btn btn-sm" :disabled="store.busy === 'vcp'" @click="vcpAction('restart')"><Refresh :size="15" />{{ t("overview.restart") }}</button>
        </div>
      </section>

      <!-- Apps -->
      <section class="card p-5">
        <div class="card-head"><Devices />{{ t("overview.appsTitle") }}</div>
        <dl class="space-y-2 text-sm">
          <div class="flex justify-between gap-3"><dt class="muted">{{ t("overview.paired") }}</dt><dd class="font-medium">{{ devices.length }}</dd></div>
          <div class="flex justify-between gap-3"><dt class="muted">{{ t("overview.connections") }}</dt><dd class="font-medium">{{ status!.connections }}</dd></div>
          <div v-if="lastDevice" class="flex justify-between gap-3">
            <dt class="muted whitespace-nowrap">{{ t("overview.lastSeen") }}</dt>
            <dd class="min-w-0 text-right font-medium"><span class="block truncate">{{ lastDevice.name }}</span><span class="block text-xs font-normal text-muted">{{ ago(lastDevice.last_seen, locale) }}</span></dd>
          </div>
        </dl>
        <div class="mt-4"><button class="btn btn-primary btn-sm" @click="openPairing()"><Link :size="15" />{{ t("overview.pair") }}</button></div>
      </section>

      <!-- Address -->
      <section class="card p-5">
        <div class="card-head"><Network />{{ t("overview.addressTitle") }}</div>
        <p class="muted mb-2 text-xs">{{ t("overview.addressHint") }}</p>
        <ul class="space-y-1">
          <li v-for="a in status!.addresses" :key="a.ip" class="flex items-center gap-1 rounded-lg bg-panel-2 py-0.5 pl-2.5 pr-0.5">
            <code class="flex-1 truncate font-mono text-sm">{{ a.ip }}:{{ config!.port }}</code>
            <UiCopy :value="`${a.ip}:${config!.port}`" />
          </li>
        </ul>
        <div class="mt-3 text-xs">
          <span class="muted">{{ t("overview.fingerprint") }}: </span>
          <code class="font-mono">{{ store.ov!.fingerprint_short }}</code>
        </div>
      </section>
    </div>

    <!-- System monitor -->
    <section class="card p-5">
      <div class="card-head">
        <Microchip />{{ t("overview.systemTitle") }}
        <span class="ml-auto text-xs font-normal text-muted">{{ t("overview.uptime", { time: duration(status!.uptime_secs) }) }}</span>
      </div>
      <div v-if="store.system" class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
        <UiStat :label="t('overview.cpu')" :value="`${Math.round(store.system.cpu_percent)} %`" :ratio="store.system.cpu_percent / 100" :tip="t('tips.cpu')"><template #icon><Microchip :size="14" /></template></UiStat>
        <UiStat
          :label="t('overview.ram')"
          :value="bytes(store.system.mem_used, locale)"
          :sub="`/ ${bytes(store.system.mem_total, locale)}`"
          :ratio="store.system.mem_total ? store.system.mem_used / store.system.mem_total : null"
        ><template #icon><Ram :size="14" /></template></UiStat>
        <template v-if="gpu">
          <UiStat :label="t('overview.gpu')" :value="gpu.usage_percent != null ? `${Math.round(gpu.usage_percent)} %` : '–'" :sub="gpu.name" :ratio="gpu.usage_percent != null ? gpu.usage_percent / 100 : null" :tip="t('tips.gpu')"><template #icon><Waveform :size="14" /></template></UiStat>
          <UiStat
            :label="t('overview.vram')"
            :value="bytes(gpu.vram_used, locale)"
            :sub="gpu.vram_total ? `/ ${bytes(gpu.vram_total, locale)}` : undefined"
            :ratio="gpu.vram_used != null && gpu.vram_total ? gpu.vram_used / gpu.vram_total : null"
            :tip="t('tips.vram')"
          ><template #icon><Ram :size="14" /></template></UiStat>
          <UiStat v-if="gpu.temperature_c != null" :label="t('overview.temp')" :value="`${gpu.temperature_c} °C`" :ratio="gpu.temperature_c / 95"><template #icon><Temperature :size="14" /></template></UiStat>
          <UiStat v-if="gpu.power_w != null" :label="t('overview.power')" :value="`${Math.round(gpu.power_w)} W`"><template #icon><Bolt :size="14" /></template></UiStat>
        </template>
      </div>
      <p v-else class="muted text-sm">{{ t("common.loading") }}</p>
      <p class="hint">{{ t("overview.systemHint") }}</p>
    </section>
  </div>
</template>
