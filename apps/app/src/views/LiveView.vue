<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import { ArrowRight, Clock, Play, Stop, Trash, Waveform, Warning } from "@ttv-tts/ui/icons";
import { reader, type ItemView } from "../api";
import { host, run, store, toast } from "../store";

const { t } = useI18n();
const r = computed(() => store.reader!);
const status = computed(() => r.value.status);
const profiles = computed(() => r.value.store.profiles);
const active = computed(() => r.value.store.active);

const testUser = ref("TestUser");
const testText = ref("");
const testProfile = ref(active.value[0] ?? profiles.value[0]?.id ?? "");

async function toggle(id: string, on: boolean) {
  const ids = on ? [...active.value, id] : active.value.filter((a) => a !== id);
  const v = await run("active", () => reader.setActive(ids));
  if (v) store.reader!.store = v.store;
  if (on && !testProfile.value) testProfile.value = id;
}

const history = computed(() => [...r.value.history].reverse());
const time = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

const stateCls: Record<ItemView["state"], string> = {
  queued: "",
  speaking: "badge-accent",
  spoken: "badge-ok",
  skipped: "badge-warn",
  removed: "badge-warn",
  failed: "badge-err",
  filtered: "",
};

async function mute(it: ItemView) {
  const pid = profiles.value.find((p) => p.channels.includes(it.channel))?.id;
  if (!pid) return;
  const v = await run("mute", () => reader.mute(pid, it.login, true));
  if (v) {
    store.reader!.store = v.store;
    toast("info", t("live.mutedToast", { name: it.display }));
  }
}

async function sendTest() {
  const text = testText.value.trim() || t("live.testTextPlaceholder");
  await run("test", () => reader.test(testProfile.value, testUser.value, text));
  testText.value = "";
}
</script>

<template>
  <div class="space-y-5">
    <p v-if="host?.kind === 'none'" class="notice notice-warn text-sm">
      <Warning :size="18" /><span class="flex-1">{{ t("live.noHost") }}</span>
      <button class="btn btn-sm" @click="store.view = 'host'">{{ t("host.setupTitle") }}</button>
    </p>

    <!-- profiles on/off + channel status -->
    <section class="card p-5">
      <div class="card-head">
        <Play />{{ t("live.reading") }}<UiTip :text="t('tips.profiles')" />
        <span class="ml-auto flex items-center gap-1.5 text-xs font-normal text-muted">
          <span class="dot" :class="status.connected ? 'dot-ok' : active.length ? 'dot-warn' : ''" />
          {{ status.connected ? t("live.connected") : active.length ? t("live.connecting") : "" }}
        </span>
      </div>
      <p class="muted mb-3 text-sm">{{ t("live.readingHint") }}</p>
      <div class="flex flex-wrap gap-3">
        <div v-for="p in profiles" :key="p.id" class="flex items-center gap-3 rounded-xl border border-line bg-panel-2 px-3.5 py-2.5" :class="active.includes(p.id) && 'border-accent'">
          <UiToggle :model-value="active.includes(p.id)" @update:model-value="(on: boolean) => toggle(p.id, on)" />
          <div class="min-w-0">
            <div class="text-sm font-semibold">{{ p.name }}</div>
            <div class="flex flex-wrap gap-x-2 text-xs text-muted">
              <span v-for="c in p.channels" :key="c" class="flex items-center gap-1">
                <span class="dot" :class="status.channels.find((x) => x.name === c)?.joined ? 'dot-ok' : active.includes(p.id) ? 'dot-warn' : ''" />#{{ c }}
              </span>
            </div>
          </div>
        </div>
      </div>
      <p v-if="!active.length" class="mt-3 text-sm text-muted">{{ t("live.off") }}</p>
      <p v-if="status.error" class="notice notice-err mt-3 text-sm">{{ status.error }}</p>
    </section>

    <div class="grid gap-5 lg:grid-cols-[3fr_2fr]">
      <!-- now speaking -->
      <section class="card p-5">
        <div class="card-head">
          <Waveform />{{ t("live.now") }}
          <div class="ml-auto flex gap-2">
            <button class="btn btn-sm" @click="reader.pause(!status.paused)">
              <Play v-if="status.paused" :size="15" /><Clock v-else :size="15" />{{ status.paused ? t("live.resume") : t("live.pause") }}
            </button>
            <button class="btn btn-sm" :disabled="!status.current" @click="reader.skip()"><ArrowRight :size="15" />{{ t("live.skip") }}</button>
            <button class="btn btn-danger btn-sm" :disabled="!status.queue.length && !status.current" @click="reader.clear()"><Trash :size="15" />{{ t("live.clear") }}</button>
          </div>
        </div>
        <div v-if="status.current" class="rounded-xl bg-accent-soft p-4">
          <div class="flex items-center gap-2 text-sm">
            <b :style="status.current.color ? { color: status.current.color } : undefined">{{ status.current.display }}</b>
            <span class="text-muted">#{{ status.current.channel }} · {{ status.current.lang }}</span>
          </div>
          <p class="mt-1.5 text-lg leading-snug">{{ status.current.speech }}</p>
          <p class="mt-2 text-xs text-muted">
            {{ status.current.voice }}<span v-if="status.current.first_audio_ms"> · {{ t("live.firstAudio", { s: (status.current.first_audio_ms / 1000).toFixed(1) }) }}</span>
          </p>
        </div>
        <p v-else class="rounded-xl bg-panel-2 p-4 text-sm text-muted">{{ status.paused ? t("live.paused") : t("live.silence") }}</p>

        <div class="mt-4">
          <div class="label">{{ t("live.queue") }} <span class="badge ml-1">{{ status.queue.length }}</span></div>
          <p v-if="!status.queue.length" class="text-sm text-muted">{{ t("live.queueEmpty") }}</p>
          <ul v-else class="space-y-1">
            <li v-for="q in status.queue" :key="q.id" class="truncate rounded-lg bg-panel-2 px-3 py-1.5 text-sm">
              <b :style="q.color ? { color: q.color } : undefined">{{ q.display }}</b> <span class="text-muted">{{ q.speech }}</span>
            </li>
          </ul>
        </div>
      </section>

      <!-- test message -->
      <section class="card p-5">
        <div class="card-head"><Play />{{ t("live.testTitle") }}</div>
        <div class="space-y-3">
          <div class="grid grid-cols-[1fr_2fr] gap-2">
            <input v-model="testUser" class="input" :placeholder="t('live.testUser')" />
            <select v-model="testProfile" class="input">
              <option v-for="p in profiles" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
          </div>
          <textarea v-model="testText" rows="3" class="input h-auto py-2" :placeholder="t('live.testTextPlaceholder')" @keydown.enter.exact.prevent="sendTest" />
          <button class="btn btn-primary" :disabled="!testProfile" @click="sendTest"><Play :size="16" />{{ t("live.testSend") }}</button>
        </div>
      </section>
    </div>

    <!-- history -->
    <section class="card p-5">
      <div class="card-head"><Clock />{{ t("live.history") }}</div>
      <p v-if="!history.length" class="text-sm text-muted">{{ t("live.historyEmpty") }}</p>
      <ul v-else class="max-h-[32rem] divide-y divide-line overflow-y-auto">
        <li v-for="h in history" :key="h.id" class="group flex items-start gap-3 py-2 text-sm">
          <span class="w-12 shrink-0 pt-0.5 text-xs tabular-nums text-muted">{{ time(h.at) }}</span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-x-2">
              <b :style="h.color ? { color: h.color } : undefined">{{ h.display }}</b>
              <span class="badge" :class="stateCls[h.state]">{{ t(`live.states.${h.state}`) }}</span>
              <span v-if="h.reason" class="text-xs text-muted">{{ h.reason }}</span>
            </div>
            <div class="truncate text-muted" :title="h.original">{{ h.speech || h.original }}</div>
          </div>
          <button class="btn btn-ghost btn-sm opacity-0 group-hover:opacity-100" @click="mute(h)"><Stop :size="14" />{{ t("live.mute") }}</button>
        </li>
      </ul>
    </section>
  </div>
</template>
