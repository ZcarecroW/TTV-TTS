<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Link, Spinner } from "@ttv-tts/ui/icons";
import { listen, openUrl, twitch, type TwitchStatus } from "../api";
import { run, store, toast } from "../store";

// Optional Twitch login: follows and channel-point rewards without text
// (EventSub). Chat itself is read anonymously either way.
const { t, locale } = useI18n();
const s = ref<TwitchStatus | null>(null);
const ownId = ref("");
let unlisten: (() => void) | null = null;
onMounted(async () => {
  s.value = await twitch.status().catch(() => null);
  unlisten = await listen<TwitchStatus>("app://twitch", (v) => {
    const was = s.value?.login;
    s.value = v;
    if (!was && v.login) toast("ok", t("settings.twitchDone", { login: v.login }));
  });
});
onBeforeUnmount(() => unlisten?.());

async function login() {
  const r = await run("twitch", () => twitch.login());
  if (r) s.value = r;
}
async function cancel() {
  s.value = await twitch.cancel();
}
async function logout() {
  const r = await run("twitch", () => twitch.logout());
  if (r) s.value = r;
}
async function saveId() {
  const r = await run("twitchId", () => twitch.setClientId(ownId.value));
  if (r) {
    s.value = r;
    ownId.value = "";
    toast("ok", t("settings.twitchIdSaved"));
  }
}
const until = computed(() => (s.value?.pending ? new Date(s.value.pending.expires_at * 1000).toLocaleTimeString(locale.value, { hour: "2-digit", minute: "2-digit" }) : ""));
const has = (kind: string) => !!s.value?.subscribed.includes(kind);
</script>

<template>
  <section class="card p-5" data-twitch>
    <div class="card-head"><Link />{{ t("settings.twitchTitle") }}<UiTip :text="t('tips.twitch')" /></div>
    <template v-if="s">
      <p class="muted mb-3 text-sm">{{ t("settings.twitchIntro") }}</p>

      <!-- logged in -->
      <div v-if="s.login" class="space-y-2 text-sm" data-twitch-account>
        <div class="flex flex-wrap items-center gap-2">
          <span class="font-medium">{{ t("settings.twitchAs", { login: s.login }) }}</span>
          <span v-if="s.connected" class="badge" :class="has('channel.follow') ? 'badge-ok' : ''">{{ t("settings.twitchFollows") }}</span>
          <span v-if="s.connected" class="badge" :class="has('channel.channel_points_custom_reward_redemption.add') ? 'badge-ok' : ''">{{ t("settings.twitchRewards") }}</span>
          <span v-if="!s.connected && !s.error" class="flex items-center gap-1.5 text-xs text-muted"><Spinner :size="13" class="spin" />{{ t("settings.twitchConnecting") }}</span>
          <button class="btn btn-sm ml-auto" :disabled="store.busy === 'twitch'" @click="logout">{{ t("settings.twitchLogout") }}</button>
        </div>
      </div>

      <!-- waiting for the code to be confirmed -->
      <div v-else-if="s.pending" class="rounded-xl border border-accent bg-accent-soft/40 p-3.5 text-sm" data-twitch-pending>
        <div class="mb-1.5">{{ t("settings.twitchCode", { time: until }) }}</div>
        <div class="flex flex-wrap items-center gap-3">
          <code class="rounded-lg bg-panel px-3 py-1.5 font-mono text-xl tracking-widest">{{ s.pending.user_code }}</code>
          <button class="btn btn-sm" @click="openUrl(s.pending.verification_uri)">{{ t("settings.twitchOpen") }}</button>
          <span class="flex items-center gap-1.5 text-xs text-muted"><Spinner :size="13" class="spin" />{{ t("settings.twitchWaiting") }}</span>
          <button class="btn btn-ghost btn-sm ml-auto" @click="cancel">{{ t("common.cancel") }}</button>
        </div>
      </div>

      <button v-else-if="s.available" class="btn btn-primary" :disabled="store.busy === 'twitch'" @click="login"><Spinner v-if="store.busy === 'twitch'" :size="14" class="spin" />{{ t("settings.twitchLogin") }}</button>
      <p v-else class="text-sm text-warn">{{ t("settings.twitchNoId") }}</p>

      <p v-if="s.error" class="mt-2 text-sm text-err" data-twitch-error>{{ s.error }}</p>

      <details class="mt-3 text-sm" :open="!s.available">
        <summary class="cursor-pointer text-muted">{{ t("settings.twitchOwnTitle") }}</summary>
        <ol class="mt-2 list-decimal space-y-1 pl-5 text-xs text-muted">
          <li>{{ t("settings.twitchGuide1") }}</li>
          <li>{{ t("settings.twitchGuide2") }}</li>
          <li>{{ t("settings.twitchGuide3") }}</li>
        </ol>
        <div class="mt-2 flex gap-2">
          <input v-model="ownId" class="input mono max-w-sm" :placeholder="s.own_client_id ? t('settings.twitchIdStored') : t('settings.twitchIdPlaceholder')" />
          <button class="btn" :disabled="store.busy === 'twitchId' || !ownId.trim()" @click="saveId">{{ t("common.save") }}</button>
          <button v-if="s.own_client_id" class="btn btn-ghost" @click="ownId = ''; saveId()">{{ t("settings.twitchIdRemove") }}</button>
        </div>
      </details>
    </template>
  </section>
</template>
