<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import { CircleInfo, Download, ExternalLink, FolderOpen, Globe, Lock, Power, Refresh } from "@ttv-tts/ui/icons";
import { copyText } from "@ttv-tts/ui/clipboard";
import { api, isTauri } from "../api";
import { config, run, save, status, store, toast } from "../store";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";

const { t, locale } = useI18n();
const upd = computed(() => status.value!.update);
const isMac = computed(() => status.value!.info.os === "macos");

const language = computed({ get: () => config.value!.language, set: (v: string) => void save({ language: v }, true) });
const theme = computed({ get: () => config.value!.theme, set: (v: string) => void save({ theme: v }, true) });
const autostart = computed({ get: () => config.value!.autostart, set: (v: boolean) => void save({ autostart: v }) });
const channel = computed({ get: () => config.value!.update_channel, set: (v: string) => void save({ update_channel: v }, true) });
const autoUpdate = computed({ get: () => config.value!.auto_update, set: (v: boolean) => void save({ auto_update: v }) });

async function checkNow() {
  await run("update", () => api.checkUpdate());
}
async function installNow() {
  toast("info", t("settings.installing"));
  await run("update", () => api.installUpdate());
}
async function resetCert() {
  if (!confirm(t("settings.resetCertConfirm"))) return;
  const ov = await run("cert", () => api.resetCertificate());
  if (ov) {
    store.ov = ov;
    toast("ok", t("settings.resetCertDone"));
  }
}
async function diagnostics() {
  const d = await run("diag", () => api.diagnostics());
  if (d && (await copyText(JSON.stringify(d, null, 2)))) toast("ok", t("settings.diagnosticsCopied"));
}
async function open(url: string) {
  if (!isTauri) return void window.open(url, "_blank");
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}
async function openData() {
  if (!isTauri) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(store.ov!.data_dir);
}
const docsUrl = computed(() => `https://zcarecrow.github.io/TTV-TTS/${locale.value === "de" ? "de/" : ""}proxy/`);
</script>

<template>
  <div class="grid gap-5 md:grid-cols-2">
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
      <div class="card-head"><Power />{{ t("settings.startTitle") }}<UiDocs class="ml-auto" page="proxy" anchor="settings" /></div>
      <UiToggle v-model="autostart" :label="isMac ? t('settings.autostartMac') : t('settings.autostart')" :tip="t('tips.autostart')" />
    </section>

    <section class="card p-5">
      <div class="card-head"><Download />{{ t("settings.updatesTitle") }}</div>
      <div class="space-y-4">
        <div class="text-sm"><span class="muted">{{ t("settings.version") }}:</span> <b>{{ status!.info.version }}</b></div>
        <UiSegmented
          v-model="channel"
          :label="t('settings.channel')"
          :tip="t('tips.channel')"
          :options="[
            { value: 'stable', label: t('settings.channelStable') },
            { value: 'preview', label: t('settings.channelPreview') },
          ]"
        />
        <UiToggle v-model="autoUpdate" :label="t('settings.autoUpdate')" :tip="t('tips.autoUpdate')" />
        <p v-if="upd?.phase === 'error'" class="notice notice-err text-sm"><CircleInfo :size="18" />{{ t("settings.checkFailed", { error: upd.error }) }}</p>
        <p v-else-if="upd?.available" class="notice notice-ok text-sm"><Download :size="18" />{{ t("settings.available", { version: upd.available }) }}</p>
        <p v-else-if="upd?.phase === 'idle'" class="text-sm text-muted">{{ t("settings.upToDate") }}</p>
        <div class="flex gap-2">
          <button class="btn btn-sm" :disabled="store.busy === 'update' || upd?.phase === 'checking'" @click="checkNow"><Refresh :size="15" :class="upd?.phase === 'checking' && 'spin'" />{{ t("settings.checkNow") }}</button>
          <button v-if="upd?.available" class="btn btn-primary btn-sm" :disabled="store.busy === 'update'" @click="installNow"><Download :size="15" />{{ t("settings.install") }}</button>
        </div>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><Lock />{{ t("settings.securityTitle") }}</div>
      <p class="muted mb-3 text-sm">{{ t("settings.resetCertText") }}</p>
      <button class="btn btn-danger btn-sm" :disabled="store.busy === 'cert'" @click="resetCert">{{ t("settings.resetCert") }}</button>
    </section>

    <section class="card p-5 md:col-span-2">
      <div class="card-head"><CircleInfo />{{ t("settings.aboutTitle") }}</div>
      <p class="text-sm">{{ t("app.name") }} {{ status!.info.version }} · MIT License · © 2026 ZcarecroW</p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button class="btn btn-sm" @click="open(docsUrl)"><ExternalLink :size="15" />{{ t("settings.docs") }}</button>
        <button class="btn btn-sm" @click="open('https://github.com/ZcarecroW/TTV-TTS')"><ExternalLink :size="15" />{{ t("settings.source") }}</button>
        <button class="btn btn-sm" @click="openData"><FolderOpen :size="15" />{{ t("settings.dataFolder") }}</button>
        <button class="btn btn-sm" :data-tip="t('settings.diagnosticsTip')" @click="diagnostics">{{ t("settings.diagnostics") }}</button>
      </div>
    </section>
  </div>
</template>
