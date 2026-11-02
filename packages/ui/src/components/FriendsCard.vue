<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "./UiTip.vue";
import UiDocs from "./UiDocs.vue";
import UiToggle from "./UiToggle.vue";
import { copyText } from "../clipboard";
import { ago } from "../format";
import { Copy, Globe, Lock, Plus, ShieldCheck, Spinner, Trash, Users, X } from "../icons";
import type { FriendsApi, InternetStatus } from "../friends";

// Friends use a proxy's voices over the internet (guest API on its own port).
// Shown in the app (for the paired proxy) and in the proxy window itself.
const props = defineProps<{
  api: FriendsApi;
  /** the window's busy marker and helpers (toasts, errors) */
  busy: string;
  run: <T>(name: string, fn: () => Promise<T>) => Promise<T | undefined>;
  toast: (kind: "ok" | "err" | "info", text: string) => void;
}>();
const { api: friends, run, toast } = props;
const { t, locale } = useI18n();
const s = ref<InternetStatus | null>(null);
const error = ref("");
const domain = ref("");
const token = ref("");
const inviteName = ref("");
const days = ref(7);
const quota = ref(30000);
const via = ref("");
const fresh = ref<{ name: string; code: string; expires: number } | null>(null);

async function load() {
  try {
    s.value = await friends.status();
    error.value = "";
    if (!domain.value) domain.value = s.value.domain;
  } catch (e) {
    error.value = String(e);
  }
}
let timer = 0;
onMounted(() => {
  void load();
  timer = window.setInterval(load, 15000);
});
onBeforeUnmount(() => clearInterval(timer));

async function toggle(on: boolean) {
  const r = await run("internet", () => friends.set({ enabled: on }));
  if (r) s.value = r;
}

async function saveName() {
  const patch: { domain: string; token?: string } = { domain: domain.value };
  if (token.value.trim()) patch.token = token.value.trim();
  const r = await run("duckdns", () => friends.set(patch));
  if (r) {
    s.value = r;
    token.value = "";
    toast(r.duckdns_result === "OK" ? "ok" : "err", r.duckdns_result === "OK" ? t("friends.duckOk", { name: `${r.domain}.duckdns.org` }) : (r.duckdns_result ?? t("friends.duckSaved")));
  }
}

async function test() {
  const r = await run("internetTest", () => friends.test());
  if (r) {
    await load();
    toast(r.reachable ? "ok" : "err", r.reachable ? t("friends.reachable") : t("friends.unreachable", { port: s.value?.port ?? 8111 }));
  }
}

async function invite() {
  const r = await run("invite", () => friends.invite(inviteName.value.trim(), days.value, quota.value, via.value.trim() || null));
  if (r) {
    fresh.value = { name: r.invite.name, code: r.code, expires: r.invite.expires };
    inviteName.value = "";
    await load();
  }
}

async function copyCode() {
  if (fresh.value && (await copyText(fresh.value.code))) toast("ok", t("friends.copied"));
}

async function withdraw(id: string) {
  await run("withdraw", () => friends.withdraw(id));
  await load();
}

async function revoke(id: string, name: string) {
  if (!confirm(t("friends.revokeConfirm", { name }))) return;
  await run("revoke", () => friends.revoke(id));
  await load();
}

const date = (secs: number) => new Date(secs * 1000).toLocaleDateString(locale.value);
const k = (n: number) => (n >= 1000 ? `${Math.round(n / 1000)}k` : String(n));
const duckLine = computed(() => {
  const x = s.value;
  if (!x?.domain) return "";
  if (!x.duckdns_at) return t("friends.duckPending");
  return x.duckdns_result === "OK" ? t("friends.duckUpdated", { ago: ago(x.duckdns_at, locale.value), ip: x.public_ip ?? "?" }) : (x.duckdns_result ?? "");
});
</script>

<template>
  <section class="card p-5">
    <div class="card-head">
      <Users />{{ t("friends.title") }}<UiTip :text="t('tips.friends')" />
      <UiDocs class="ml-auto" page="friends" />
      <span v-if="s?.enabled" class="badge" :class="s.listening ? 'badge-ok' : 'badge-err'">{{ s.listening ? t("friends.on", { port: s.port }) : t("friends.portError") }}</span>
    </div>
    <p v-if="error" class="text-sm text-muted">{{ error }}</p>
    <template v-else-if="s">
      <p class="muted mb-3 text-sm">{{ t("friends.intro") }}</p>
      <UiToggle :model-value="s.enabled" :disabled="busy === 'internet'" :label="t('friends.enable')" :hint="t('friends.enableHint', { port: s.port })" @update:model-value="toggle" />
      <p v-if="s.error" class="mt-2 text-sm text-err">{{ s.error }}</p>

      <div v-if="s.enabled" class="mt-5 space-y-5">
        <!-- 1. a name -->
        <div>
          <div class="label">1 · {{ t("friends.nameTitle") }}<UiTip :text="t('tips.duckdns')" /></div>
          <div class="grid gap-2 sm:grid-cols-[1fr_1fr_auto]">
            <div class="flex items-center gap-1"><input v-model="domain" class="input mono" :placeholder="t('friends.domainPlaceholder')" /><span class="shrink-0 text-sm text-muted">.duckdns.org</span></div>
            <input v-model="token" type="password" autocomplete="off" class="input mono" :placeholder="s.duckdns_token ? t('friends.tokenStored') : t('friends.tokenPlaceholder')" />
            <button class="btn" :disabled="busy === 'duckdns' || !domain.trim()" @click="saveName"><Spinner v-if="busy === 'duckdns'" :size="14" class="spin" />{{ t("common.save") }}</button>
          </div>
          <p class="hint mt-1.5">{{ duckLine || t("friends.duckHint") }}</p>
        </div>

        <!-- 2. router + firewall -->
        <details class="rounded-xl border border-line p-3.5 text-sm">
          <summary class="cursor-pointer font-medium">2 · {{ t("friends.routerTitle", { port: s.port }) }}</summary>
          <ol class="mt-2.5 list-decimal space-y-1.5 pl-5 text-muted">
            <li>{{ t("friends.router1", { port: s.port }) }}</li>
            <li>{{ t("friends.router2", { port: s.port }) }}</li>
            <li>{{ t("friends.router3", { port: s.port }) }}</li>
            <li>{{ t("friends.router4") }}</li>
          </ol>
        </details>

        <!-- 3. check -->
        <div class="flex flex-wrap items-center gap-3">
          <button class="btn btn-sm" :disabled="busy === 'internetTest'" @click="test"><Spinner v-if="busy === 'internetTest'" :size="14" class="spin" /><Globe v-else :size="14" />3 · {{ t("friends.test") }}</button>
          <span v-if="s.test_at" class="text-sm" :class="s.reachable ? 'text-ok' : 'text-warn'">{{ s.reachable ? t("friends.reachableShort", { address: s.address }) : t("friends.unreachableShort") }} · {{ ago(s.test_at, locale) }}</span>
          <UiTip :text="t('tips.internetTest')" />
        </div>

        <!-- 4. invites -->
        <div>
          <div class="label">4 · {{ t("friends.inviteTitle") }}</div>
          <div class="grid gap-2 sm:grid-cols-[1fr_auto_auto_auto]">
            <input v-model="inviteName" class="input" :placeholder="t('friends.invitePlaceholder')" @keydown.enter="inviteName.trim() && invite()" />
            <select v-model.number="days" class="input" :aria-label="t('friends.valid')">
              <option :value="1">{{ t("friends.days", 1) }}</option>
              <option :value="7">{{ t("friends.days", 7) }}</option>
              <option :value="30">{{ t("friends.days", 30) }}</option>
            </select>
            <select v-model.number="quota" class="input" :aria-label="t('friends.quota')">
              <option :value="10000">{{ t("friends.perDay", { n: "10k" }) }}</option>
              <option :value="30000">{{ t("friends.perDay", { n: "30k" }) }}</option>
              <option :value="100000">{{ t("friends.perDay", { n: "100k" }) }}</option>
            </select>
            <button class="btn btn-primary" :disabled="!inviteName.trim() || busy === 'invite'" @click="invite"><Plus :size="15" />{{ t("friends.invite") }}</button>
          </div>
          <details class="mt-1.5 text-xs text-muted">
            <summary class="cursor-pointer">{{ t("friends.viaTitle") }}</summary>
            <input v-model="via" class="input mono mt-1.5 max-w-md" :placeholder="s.address || t('friends.viaPlaceholder')" />
            <p class="mt-1">{{ t("friends.viaHint") }}</p>
          </details>
          <div v-if="fresh" class="mt-3 rounded-xl border border-accent bg-accent-soft/40 p-3.5 text-sm">
            <div class="mb-1.5 flex items-center gap-2 font-medium"><Lock :size="15" />{{ t("friends.codeFor", { name: fresh.name, date: date(fresh.expires) }) }}</div>
            <div class="flex gap-2">
              <code class="min-w-0 flex-1 truncate rounded-lg bg-panel px-2 py-1.5 font-mono text-xs">{{ fresh.code }}</code>
              <button class="btn btn-sm" @click="copyCode"><Copy :size="14" />{{ t("friends.copy") }}</button>
              <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('common.close')" @click="fresh = null"><X :size="14" /></button>
            </div>
            <p class="mt-1.5 text-xs text-muted">{{ t("friends.codeHint") }}</p>
          </div>
          <ul v-if="s.invites.length" class="mt-3 space-y-1 text-sm">
            <li v-for="i in s.invites" :key="i.id" class="flex items-center gap-2">
              <span class="badge">{{ t("friends.open") }}</span>{{ i.name }}<span class="text-xs text-muted">{{ t("friends.until", { date: date(i.expires) }) }} · {{ t("friends.perDay", { n: k(i.chars_per_day) }) }}</span>
              <button class="btn btn-ghost btn-sm ml-auto" @click="withdraw(i.id)">{{ t("friends.withdraw") }}</button>
            </li>
          </ul>
        </div>
      </div>

      <!-- guests (also while switched off: they can be removed) -->
      <div v-if="s.guests.length" class="mt-5">
        <div class="label">{{ t("friends.guests") }}</div>
        <ul class="divide-y divide-line rounded-xl border border-line text-sm">
          <li v-for="g in s.guests" :key="g.id" class="flex flex-wrap items-center gap-x-3 gap-y-1 px-3.5 py-2.5">
            <span class="font-medium">{{ g.name }}</span>
            <span class="text-xs text-muted">{{ g.last_seen ? t("friends.seen", { ago: ago(g.last_seen, locale) }) : "" }}<template v-if="g.last_address"> · {{ g.last_address }}</template></span>
            <span class="ml-auto text-xs tabular-nums" :class="g.chars_today >= g.chars_per_day ? 'text-warn' : 'text-muted'">{{ t("friends.used", { n: k(g.chars_today), max: k(g.chars_per_day) }) }}</span>
            <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('friends.revoke')" :data-tip="t('friends.revoke')" @click="revoke(g.id, g.name)"><Trash :size="15" /></button>
          </li>
        </ul>
      </div>
      <p class="mt-4 flex gap-2 text-xs text-muted"><ShieldCheck :size="15" class="shrink-0" />{{ t("friends.safety") }}</p>
    </template>
    <p v-else class="text-sm text-muted"><Spinner :size="14" class="spin" /> {{ t("common.loading") }}</p>
  </section>
</template>
