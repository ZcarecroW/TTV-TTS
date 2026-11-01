<script setup lang="ts">
import { computed, type Component } from "vue";
import { useI18n } from "vue-i18n";
import TipLayer from "@ttv-tts/ui/components/TipLayer.vue";
import { CircleCheck, CircleInfo, CircleX, Gear, Play, Server, Waveform } from "@ttv-tts/ui/icons";
import logo from "./assets/logo.svg";
import { host, store, type View } from "./store";
import LiveView from "./views/LiveView.vue";
import VoicesView from "./views/VoicesView.vue";
import HostView from "./views/HostView.vue";
import SettingsView from "./views/SettingsView.vue";

const { t } = useI18n();

const views: Record<View, Component> = { live: LiveView, voices: VoicesView, host: HostView, settings: SettingsView };

const hostDot = computed(() => {
  const h = host.value;
  if (!h || h.kind === "none") return "dot-warn";
  if (!h.connected) return "dot-err";
  return h.vcp?.reachable ? "dot-ok" : "dot-warn";
});

const nav = computed(() => [
  { id: "live" as View, icon: Play, dot: "" },
  { id: "voices" as View, icon: Waveform, dot: "" },
  { id: "host" as View, icon: Server, dot: hostDot.value },
  { id: "settings" as View, icon: Gear, dot: "" },
]);
</script>

<template>
  <div v-if="store.app" class="flex h-full">
    <aside class="flex w-56 shrink-0 flex-col border-r border-line bg-panel/60 p-3">
      <div class="mb-5 flex items-center gap-2.5 px-2 pt-1">
        <img :src="logo" alt="" width="36" height="36" class="rounded-lg" />
        <div class="min-w-0">
          <div class="text-[0.95rem] font-semibold leading-tight">{{ t("app.name") }}</div>
          <div class="text-xs leading-tight text-muted">{{ t("app.tagline") }}</div>
        </div>
      </div>
      <nav class="flex flex-col gap-0.5" :aria-label="t('app.name')">
        <button
          v-for="n in nav"
          :key="n.id"
          type="button"
          class="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-sm font-medium transition-colors"
          :class="store.view === n.id ? 'bg-accent-soft text-accent-text' : 'text-muted hover:bg-panel-2 hover:text-fg'"
          :aria-current="store.view === n.id ? 'page' : undefined"
          @click="store.view = n.id"
        >
          <component :is="n.icon" :size="18" />
          <span class="flex-1 truncate">{{ t(`nav.${n.id}`) }}</span>
          <span v-if="n.dot" class="dot" :class="n.dot" />
        </button>
      </nav>
      <div class="mt-auto px-2 pt-4 text-xs text-muted">
        <div v-if="host && host.kind !== 'none'" class="flex items-center gap-1.5">
          <span class="dot" :class="hostDot" />
          <span class="truncate">{{ host.kind === "proxy" ? host.proxy?.info.name ?? t("host.proxy") : t("host.direct") }}</span>
        </div>
        <div class="mt-1">v{{ store.app.version }}</div>
      </div>
    </aside>

    <main class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-5xl px-7 py-6">
        <h1 class="mb-5 text-xl font-semibold tracking-tight">{{ t(`nav.${store.view}`) }}</h1>
        <component :is="views[store.view]" />
      </div>
    </main>
  </div>
  <div v-else class="flex h-full items-center justify-center text-muted">{{ t("common.loading") }}</div>

  <Transition enter-from-class="opacity-0 translate-y-2" leave-to-class="opacity-0" enter-active-class="transition" leave-active-class="transition">
    <div v-if="store.toast" role="status" class="card fixed bottom-5 left-1/2 z-[60] flex max-w-lg -translate-x-1/2 items-start gap-2.5 px-4 py-3 text-sm">
      <CircleCheck v-if="store.toast.kind === 'ok'" class="text-ok" />
      <CircleX v-else-if="store.toast.kind === 'err'" class="text-err" />
      <CircleInfo v-else class="text-accent-text" />
      <span>{{ store.toast.text }}</span>
    </div>
  </Transition>
  <TipLayer />
</template>
