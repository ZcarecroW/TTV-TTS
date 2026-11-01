<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { CircleInfo, Copy, Download, ExternalLink, Globe, Keyboard, Link, Sparkle, Spinner, Waveform } from "@ttv-tts/ui/icons";
import { api, HOTKEY_ACTIONS, isTauri, updates, type Measured } from "../api";
import { apply, config, run, store, toast } from "../store";
import { copyText } from "@ttv-tts/ui/clipboard";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";

const { t, locale } = useI18n();
const c = computed(() => config.value!);

const device = computed({
  get: () => c.value.audio.device ?? "",
  set: (v: string) => void run("audio", () => api.setAudio({ device: v || null })).then(apply),
});
// Volume and gap are saved when the slider is released, not on every step.
const volume = ref(Math.round(c.value.audio.volume * 100));
const gap = ref(c.value.audio.gap_ms);
watch(() => c.value.audio.volume, (v) => (volume.value = Math.round(v * 100)));
const saveVolume = () => void run("audio", () => api.setAudio({ volume: volume.value / 100 })).then(apply);
const saveGap = () => void run("audio", () => api.setAudio({ gap_ms: gap.value })).then(apply);

const setting = <K extends "language" | "theme" | "update_channel" | "generation">(key: K) =>
  computed({ get: () => c.value[key] as string, set: (v: string) => void run("settings", () => api.updateSettings({ [key]: v } as never)).then(apply) });
const language = setting("language");
const theme = setting("theme");
const channel = setting("update_channel");
const generation = setting("generation");

const email = ref(c.value.translate_email ?? "");
const saveEmail = () => {
  if (email.value.trim() === (c.value.translate_email ?? "")) return;
  void run("settings", () => api.updateSettings({ translate_email: email.value.trim() })).then(apply);
};
const deepl = ref("");
async function saveDeepl() {
  const v = await run("settings", () => api.setDeeplKey(deepl.value.trim()));
  if (v) {
    apply(v);
    deepl.value = "";
  }
}
const removeDeepl = () => void run("settings", () => api.setDeeplKey(null)).then(apply);

// Generation: what "automatic" measured on this voice host.
const measured = ref<Measured | null>(null);
void api.generationInfo().then((m) => (measured.value = m)).catch(() => {});
async function measure() {
  const m = await run("measure", () => api.measureGeneration());
  if (m) measured.value = m;
}
const sec = (ms: number) => (ms / 1000).toFixed(1);

// Remote control: Stream Deck / Streamer.bot call these URLs.
const remote = computed(() => c.value.remote);
const remoteState = ref<{ listening: string | null; error: string | null }>({ listening: null, error: null });
const refreshRemote = async () => (remoteState.value = (await api.remoteStatus().catch(() => null)) ?? remoteState.value);
void refreshRemote();
const port = ref(c.value.remote?.port ?? 8130);
async function saveRemote(patch: Partial<{ enabled: boolean; port: number; lan: boolean }>, newToken = false) {
  const r = { ...remote.value, ...patch };
  const v = await run("remote", () => api.setRemote(r.enabled, r.port, r.lan, newToken));
  if (v) apply(v);
  await refreshRemote();
}
const remoteBase = computed(() => `http://127.0.0.1:${remote.value?.port ?? 8130}`);
const remoteUrls = computed(() =>
  ["skip", "toggle-pause", "clear", "replay", "approve", "reject"].map((a) => [a, `${remoteBase.value}/api/${a}?token=${remote.value?.token ?? ""}`] as const),
);
const sayUrl = computed(() => `${remoteBase.value}/api/say?token=${remote.value?.token ?? ""}&user=StreamDeck&text=Hello`);
async function copy(text: string) {
  if (await copyText(text)) toast("ok", t("settings.copied"));
}

// Hotkeys: press the combination while the field listens.
const recording = ref<string | null>(null);
const shown = (accel: string | undefined) => (accel ? accel.replace(/Key([A-Z])/g, "$1").replace(/Digit(\d)/g, "$1").replace(/\+/g, " + ") : "");
async function record(action: string, e: KeyboardEvent) {
  if (recording.value !== action) return;
  e.preventDefault();
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;
  if (e.key === "Escape") return void (recording.value = null);
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
  const fkey = /^F\d{1,2}$/.test(e.code);
  if (!mods.length && !fkey) return void toast("info", t("settings.hotkeyNeedsModifier"));
  recording.value = null;
  const v = await run("hotkey", () => api.setHotkey(action, [...mods, e.code].join("+")));
  if (v) apply(v);
}
const clearHotkey = (action: string) => void run("hotkey", () => api.setHotkey(action, null)).then(apply);

async function open(url: string) {
  if (!isTauri) return void window.open(url, "_blank");
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}
const docsUrl = computed(() => `https://zcarecrow.github.io/TTV-TTS/${locale.value === "de" ? "de/" : ""}`);
</script>

<template>
  <div class="grid gap-5 md:grid-cols-2">
    <section class="card p-5">
      <div class="card-head"><Waveform />{{ t("settings.audioTitle") }}</div>
      <div class="space-y-4">
        <UiField :label="t('settings.device')" :tip="t('tips.device')">
          <select v-model="device" class="input">
            <option value="">{{ t("settings.deviceDefault") }}</option>
            <option v-for="d in store.app!.devices" :key="d.id" :value="d.id">{{ d.name }}</option>
          </select>
        </UiField>
        <div>
          <label class="label">{{ t("settings.volume") }}<UiTip :text="t('tips.volume')" /><span class="ml-auto font-normal tabular-nums text-muted">{{ volume }} %</span></label>
          <input v-model.number="volume" type="range" min="0" max="100" class="w-full accent-[var(--accent)]" @change="saveVolume" />
        </div>
        <div>
          <label class="label">{{ t("settings.gap") }}<UiTip :text="t('tips.gap')" /><span class="ml-auto font-normal tabular-nums text-muted">{{ gap }} ms</span></label>
          <input v-model.number="gap" type="range" min="0" max="1500" step="50" class="w-full accent-[var(--accent)]" @change="saveGap" />
        </div>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><Sparkle />{{ t("settings.generationTitle") }}</div>
      <UiSegmented
        v-model="generation"
        :label="t('settings.generation')"
        :tip="t('tips.generation')"
        :options="[
          { value: 'auto', label: t('settings.genAuto') },
          { value: 'sentences', label: t('settings.genSentences') },
          { value: 'streaming', label: t('settings.genStreaming') },
        ]"
      />
      <div class="mt-3 flex flex-wrap items-center gap-2 text-sm" data-measured>
        <span v-if="measured" class="text-muted">
          {{ t("settings.genMeasured", { s1: sec(measured.sentences_first_ms), r1: measured.sentences_rtf.toFixed(2) }) }}
          · {{ measured.streaming_first_ms != null ? t("settings.genMeasuredStream", { s2: sec(measured.streaming_first_ms), r2: (measured.streaming_rtf ?? 0).toFixed(2) }) : t("settings.genStreamingFailed") }}
          → <b class="text-fg">{{ measured.pick === "streaming" ? t("settings.genStreaming") : t("settings.genSentences") }}</b>
        </span>
        <span v-else class="text-muted">{{ t("settings.genNotMeasured") }}</span>
        <button class="btn btn-sm ml-auto" :disabled="store.busy === 'measure' || !store.app?.host.connected" @click="measure">
          <Spinner v-if="store.busy === 'measure'" :size="14" class="spin" />{{ t("settings.genMeasure") }}
        </button>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><Globe />{{ t("settings.lookTitle") }}</div>
      <div class="space-y-4">
        <UiField :label="t('settings.language')">
          <select v-model="language" class="input max-w-xs">
            <option value="system">{{ t("settings.langSystem") }}</option>
            <option value="en">English</option>
            <option value="de">Deutsch</option>
          </select>
        </UiField>
        <UiSegmented
          v-model="theme"
          :label="t('settings.theme')"
          :options="[
            { value: 'system', label: t('settings.themeSystem') },
            { value: 'dark', label: t('settings.themeDark') },
            { value: 'light', label: t('settings.themeLight') },
          ]"
        />
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><Download />{{ t("settings.updatesTitle") }}</div>
      <UiSegmented
        v-model="channel"
        :label="t('settings.channel')"
        :tip="t('tips.channel')"
        :options="[
          { value: 'stable', label: t('settings.channelStable') },
          { value: 'preview', label: t('settings.channelPreview') },
        ]"
      />
      <div class="mt-4 flex flex-wrap items-center gap-2 text-sm">
        <span class="text-muted">v{{ store.app!.version }}</span>
        <span v-if="store.update?.phase === 'error'" class="text-err">{{ t("settings.checkFailed", { error: store.update.error }) }}</span>
        <span v-else-if="store.update?.available" class="text-ok">{{ t("settings.updateAvailable", { version: store.update.available }) }}</span>
        <span v-else-if="store.update?.phase === 'idle'" class="text-muted">· {{ t("settings.upToDate") }}</span>
        <button class="btn btn-sm ml-auto" :disabled="store.update?.phase === 'checking'" @click="updates.check()">{{ t("settings.checkNow") }}</button>
        <button v-if="store.update?.available" class="btn btn-primary btn-sm" @click="updates.install()">{{ t("settings.install") }}</button>
      </div>
    </section>

    <section class="card p-5" data-remote>
      <div class="card-head"><Link />{{ t("settings.remoteTitle") }}<UiTip :text="t('tips.remote')" /></div>
      <div class="space-y-3">
        <UiToggle :model-value="remote.enabled" :label="t('settings.remoteOn')" @update:model-value="saveRemote({ enabled: $event })" />
        <template v-if="remote.enabled">
          <div class="flex flex-wrap items-center gap-3 text-sm">
            <label class="flex items-center gap-2 text-muted">{{ t("settings.remotePort") }}<input v-model.number="port" type="number" min="1024" max="65535" class="input h-8 w-24" @change="saveRemote({ port })" /></label>
            <UiToggle :model-value="remote.lan" :label="t('settings.remoteLan')" :tip="t('tips.remoteLan')" @update:model-value="saveRemote({ lan: $event })" />
          </div>
          <p class="text-xs" :class="remoteState.error ? 'text-err' : 'text-ok'" data-remote-state>{{ remoteState.error ?? (remoteState.listening ? t("settings.remoteListening", { addr: remoteState.listening }) : "") }}</p>
          <div class="space-y-1">
            <div v-for="[a, url] in remoteUrls" :key="a" class="flex items-center gap-2 text-xs">
              <span class="w-28 shrink-0 text-muted">{{ t(`settings.remoteAction.${a}`) }}</span>
              <code class="min-w-0 flex-1 truncate rounded bg-panel-2 px-2 py-1">{{ url }}</code>
              <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.copy')" :data-tip="t('settings.copy')" @click="copy(url)"><Copy :size="14" /></button>
            </div>
            <div class="flex items-center gap-2 text-xs">
              <span class="w-28 shrink-0 text-muted">{{ t("settings.remoteAction.say") }}</span>
              <code class="min-w-0 flex-1 truncate rounded bg-panel-2 px-2 py-1">{{ sayUrl }}</code>
              <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('settings.copy')" :data-tip="t('settings.copy')" @click="copy(sayUrl)"><Copy :size="14" /></button>
            </div>
          </div>
          <button class="btn btn-sm" @click="saveRemote({}, true)">{{ t("settings.remoteNewToken") }}</button>
        </template>
      </div>
    </section>

    <section class="card p-5" data-hotkeys>
      <div class="card-head"><Keyboard />{{ t("settings.hotkeysTitle") }}<UiTip :text="t('tips.hotkeys')" /></div>
      <div class="space-y-2">
        <div v-for="a in HOTKEY_ACTIONS" :key="a" class="flex items-center gap-2 text-sm">
          <span class="w-36 shrink-0">{{ t(`settings.hotkey.${a}`) }}</span>
          <button
            class="input h-8 flex-1 py-0 text-left font-mono text-xs"
            :class="recording === a && 'ring-2 ring-[var(--accent)]'"
            :data-hotkey="a"
            @click="recording = recording === a ? null : a"
            @keydown="record(a, $event)"
            @blur="recording === a && (recording = null)"
          >
            {{ recording === a ? t("settings.hotkeyPress") : shown(c.hotkeys?.[a]) || t("settings.hotkeyNone") }}
          </button>
          <button v-if="c.hotkeys?.[a]" class="btn btn-ghost btn-sm" @click="clearHotkey(a)">{{ t("settings.hotkeyClear") }}</button>
        </div>
      </div>
    </section>

    <section class="card p-5" data-translation-service>
      <div class="card-head"><Globe />{{ t("settings.translateTitle") }}<UiTip :text="t('tips.translateService')" /></div>
      <div class="space-y-4">
        <UiField :label="t('settings.translateEmail')" :tip="t('tips.translateEmail')">
          <input v-model="email" class="input" type="email" autocomplete="email" :placeholder="t('settings.translateEmailPlaceholder')" @blur="saveEmail" @keydown.enter="saveEmail" />
        </UiField>
        <UiField :label="t('settings.deeplKey')" :tip="t('tips.deeplKey')">
          <div class="flex gap-2">
            <input v-model="deepl" class="input mono" type="password" autocomplete="off" :placeholder="store.app?.deepl_key ? t('settings.deeplStored') : t('settings.deeplPlaceholder')" @keydown.enter="saveDeepl" />
            <button class="btn" :disabled="!deepl.trim()" @click="saveDeepl">{{ t("common.save") }}</button>
            <button v-if="store.app?.deepl_key" class="btn btn-ghost" @click="removeDeepl">{{ t("settings.deeplRemove") }}</button>
          </div>
        </UiField>
      </div>
    </section>

    <section class="card p-5 md:col-span-2">
      <div class="card-head"><CircleInfo />{{ t("settings.aboutTitle") }}</div>
      <p class="text-sm">TTV-TTS {{ store.app!.version }} · MIT License · © 2026 ZcarecroW</p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button class="btn btn-sm" @click="open(docsUrl)"><ExternalLink :size="15" />{{ t("settings.docs") }}</button>
        <button class="btn btn-sm" @click="open('https://github.com/ZcarecroW/TTV-TTS')"><ExternalLink :size="15" />{{ t("settings.source") }}</button>
      </div>
    </section>
  </div>
</template>
