<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { ArrowRight, Clock, Play, Server, Trash } from "@ttv-tts/ui/icons";
import { hosted, listen, type HostedView, type ItemView, type ReaderStatus } from "../api";
import { reasonText } from "../reasons";
import { run } from "../store";

// Profiles the voice host PC reads for OBS: what it's saying, controls, and
// a test message – live through the proxy's events.
const { t } = useI18n();
const list = ref<HostedView[]>([]);
const recent = reactive<Record<string, ItemView[]>>({});
const error = ref("");
const testText = reactive<Record<string, string>>({});

async function load() {
  try {
    list.value = await hosted.list();
    error.value = "";
    for (const h of list.value) {
      if (!recent[h.id]) recent[h.id] = ((await hosted.history(h.id).catch(() => [])) ?? []).slice(-5).reverse();
    }
  } catch (e) {
    error.value = String(e);
  }
}

let timer = 0;
let unlisten: (() => void) | null = null;
onMounted(async () => {
  await load();
  timer = window.setInterval(load, 10_000);
  unlisten = await listen<{ id: string; status: ReaderStatus | null; item: ItemView | null }>("app://hosted", (e) => {
    const h = list.value.find((x) => x.id === e.id);
    if (h && e.status) h.status = e.status;
    if (e.item) {
      const l = (recent[e.id] ??= []);
      const i = l.findIndex((x) => x.id === e.item!.id);
      if (i >= 0) l[i] = e.item;
      else l.unshift(e.item);
      l.splice(5);
    }
  });
});
onBeforeUnmount(() => {
  clearInterval(timer);
  unlisten?.();
});

const control = (id: string, action: "skip" | "clear" | "pause" | "resume") => run("hosted", () => hosted.control(id, action));
async function test(h: HostedView) {
  const text = (testText[h.id] ?? "").trim() || t("live.testTextPlaceholder");
  const ok = await run("hosted", () => hosted.test(h.id, "TestUser", text).then(() => true));
  if (ok) testText[h.id] = "";
}
</script>

<template>
  <section class="card p-5" data-hosted>
    <div class="card-head"><Server />{{ t("hosted.title") }}<UiTip :text="t('tips.hosted')" /><UiDocs class="ml-auto" page="obs" anchor="host" /></div>
    <p v-if="error" class="notice notice-err text-sm">{{ error }}</p>
    <p v-else-if="!list.length" class="text-sm text-muted">{{ t("output.handingOver") }}</p>
    <div class="space-y-4">
      <div v-for="h in list" :key="h.id" class="rounded-xl border border-line bg-panel-2 p-4">
        <div class="flex flex-wrap items-center gap-2">
          <b>{{ h.name }}</b>
          <span class="text-xs text-muted">{{ h.channels.map((c) => `#${c}`).join(" ") }}</span>
          <span class="badge" :class="h.listeners ? 'badge-ok' : ''">{{ h.listeners ? t("output.listening", { n: h.listeners }) : t("output.notListening") }}</span>
          <span v-if="h.reading" class="badge badge-accent">{{ t("output.reading") }}</span>
          <div class="ml-auto flex gap-1.5">
            <button class="btn btn-sm" :disabled="!h.reading" @click="control(h.id, h.status.paused ? 'resume' : 'pause')">
              <Play v-if="h.status.paused" :size="15" /><Clock v-else :size="15" />{{ h.status.paused ? t("live.resume") : t("live.pause") }}
            </button>
            <button class="btn btn-sm" :disabled="!h.status.current" @click="control(h.id, 'skip')"><ArrowRight :size="15" />{{ t("live.skip") }}</button>
            <button class="btn btn-danger btn-sm" :disabled="!h.status.queue.length && !h.status.current" @click="control(h.id, 'clear')"><Trash :size="15" />{{ t("live.clear") }}</button>
          </div>
        </div>
        <p v-if="h.status.current" class="mt-2 text-sm">
          <b :style="h.status.current.color ? { color: h.status.current.color } : undefined">{{ h.status.current.display }}</b> {{ h.status.current.speech }}
        </p>
        <p v-else class="mt-2 text-sm text-muted">{{ h.reading ? t("live.silence") : t("hosted.idle") }}</p>
        <ul v-if="recent[h.id]?.length" class="mt-2 space-y-0.5 text-xs text-muted">
          <li v-for="i in recent[h.id]" :key="i.id" class="truncate">
            <span class="badge mr-1">{{ t(`live.states.${i.state}`) }}</span><b>{{ i.display }}</b> {{ i.speech || i.original }}<span v-if="i.reason"> · {{ reasonText(i.reason) }}</span>
          </li>
        </ul>
        <div class="mt-3 flex gap-2">
          <input v-model="testText[h.id]" class="input flex-1" :placeholder="t('live.testTextPlaceholder')" @keydown.enter.prevent="test(h)" />
          <button class="btn btn-sm" :disabled="!h.reading" :data-tip="h.reading ? undefined : t('hosted.testNeedsObs')" @click="test(h)"><Play :size="15" />{{ t("live.testSend") }}</button>
        </div>
      </div>
    </div>
  </section>
</template>
