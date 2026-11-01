<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import { Copy, FolderOpen } from "@ttv-tts/ui/icons";
import { copyText } from "@ttv-tts/ui/clipboard";
import { api, isTauri, type LogLine } from "../api";
import { store, toast } from "../store";

const { t } = useI18n();
const lines = ref<LogLine[]>([]);
const filter = ref("all");
let timer = 0;

async function load() {
  lines.value = await api.logs();
}
// The log is small (500 lines); polling while visible is cheaper than a stream.
onMounted(() => {
  void load();
  timer = window.setInterval(load, 2000);
});
onBeforeUnmount(() => clearInterval(timer));

const shown = computed(() => {
  const l = filter.value === "errors" ? lines.value.filter((x) => x.level === "error") : filter.value === "warnings" ? lines.value.filter((x) => x.level !== "info" && x.level !== "debug") : lines.value;
  return [...l].reverse();
});

const time = (ms: number) => new Date(ms).toLocaleTimeString();

async function copyAll() {
  const text = lines.value.map((l) => `${new Date(l.t).toISOString()} ${l.level.toUpperCase()} ${l.message}`).join("\n");
  if (await copyText(text)) toast("ok", t("common.copied"));
}

async function openFolder() {
  const path = store.ov?.log_path;
  if (!path || !isTauri) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
}
</script>

<template>
  <section class="card flex flex-col p-5">
    <div class="mb-3 flex flex-wrap items-center gap-2">
      <UiSegmented
        v-model="filter"
        :options="[
          { value: 'all', label: t('log.all') },
          { value: 'warnings', label: t('log.warnings') },
          { value: 'errors', label: t('log.errors') },
        ]"
      />
      <button class="btn btn-sm ml-auto" @click="copyAll"><Copy :size="15" />{{ t("log.copyAll") }}</button>
      <button class="btn btn-sm" @click="openFolder"><FolderOpen :size="15" />{{ t("log.openFolder") }}</button>
    </div>
    <p v-if="!shown.length" class="muted text-sm">{{ t("log.empty") }}</p>
    <ol v-else class="max-h-[calc(100vh-15rem)] overflow-y-auto rounded-xl border border-line bg-panel-2 p-2 font-mono text-xs leading-relaxed">
      <li v-for="(l, i) in shown" :key="i" class="flex gap-2.5 rounded px-1.5 py-0.5 hover:bg-panel-3">
        <span class="shrink-0 text-muted tabular-nums">{{ time(l.t) }}</span>
        <span class="w-11 shrink-0 font-semibold uppercase" :class="l.level === 'error' ? 'text-err' : l.level === 'warn' ? 'text-warn' : 'text-muted'">{{ l.level }}</span>
        <span class="min-w-0 break-words">{{ l.message }}</span>
      </li>
    </ol>
  </section>
</template>
