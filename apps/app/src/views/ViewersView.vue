<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { ago } from "@ttv-tts/ui/format";
import { Eye, EyeSlash, Refresh, Trash, Users } from "@ttv-tts/ui/icons";
import { EFFECTS, presets, reader, viewers, type ViewerView } from "../api";
import { run, store, toast } from "../store";

// Who chatted, which voice they got – reassign, mute or forget them.
const { t, locale } = useI18n();
const profiles = computed(() => store.reader?.store.profiles ?? []);
const profileId = ref(profiles.value[0]?.id ?? "");
const profile = computed(() => profiles.value.find((p) => p.id === profileId.value));
const query = ref("");
const rows = ref<ViewerView[]>([]);
const total = ref(0);
/** Voice names of the profile's preset (its first N), for reassigning. */
const voiceNames = ref<string[]>([]);

async function load() {
  if (!profileId.value) return;
  const r = await viewers.list(profileId.value, query.value).catch(() => null);
  if (r) {
    rows.value = r.viewers;
    total.value = r.total;
  }
}

async function loadVoices() {
  voiceNames.value = [];
  const p = profile.value;
  if (!p || p.voices.mode !== "preset") return;
  const d = await presets.get(p.voices.preset).catch(() => null);
  if (!d) return;
  const n = p.voices.preset_voices ? Math.min(p.voices.preset_voices, d.voices.length) : d.voices.length;
  voiceNames.value = d.voices.slice(0, n).map((v) => v.name);
}

let timer = 0;
onMounted(() => {
  void load();
  void loadVoices();
  // New chatters show up while the page is open (cheap: one local call).
  timer = window.setInterval(load, 10000);
});
onBeforeUnmount(() => clearInterval(timer));
watch(profileId, () => {
  void load();
  void loadVoices();
});
let qTimer = 0;
watch(query, () => {
  clearTimeout(qTimer);
  qTimer = window.setTimeout(load, 200);
});

const preset = computed(() => profile.value?.voices.mode === "preset");

async function reassign(v: ViewerView, role: string) {
  const r = role === "" ? null : Number(role);
  await run("viewer", () => viewers.setRole(profileId.value, v.user_id, r));
  toast("ok", r === null ? t("viewers.newRandom", { name: v.display }) : t("viewers.reassigned", { name: v.display, voice: voiceNames.value[r] ?? `#${r + 1}` }));
  await load();
}

const fxName = (f: string | null) => (f ? t(`fx.names.${f}`) : "");

async function setFx(v: ViewerView, value: string) {
  const fx = value || null;
  await run("viewer", () => viewers.setFx(profileId.value, v.user_id, fx));
  toast("ok", fx && fx !== "none" ? t("fx.set", { name: v.display, fx: fxName(fx) }) : fx ? t("fx.offFor", { name: v.display }) : t("fx.cleared", { name: v.display }));
  await load();
}

async function mute(v: ViewerView) {
  const view = await run("viewer", () => reader.mute(profileId.value, v.login, !v.muted));
  if (view) store.reader = view;
  await load();
}

async function forget(v: ViewerView) {
  if (!confirm(t("viewers.forgetConfirm", { name: v.display }))) return;
  await run("viewer", () => viewers.forget(profileId.value, v.user_id));
  await load();
}
</script>

<template>
  <div class="space-y-5">
    <section class="card p-5">
      <div class="card-head">
        <Users />{{ t("viewers.title") }}<span class="badge ml-1">{{ total }}</span><UiTip :text="t('tips.viewers')" />
        <button class="btn btn-ghost btn-sm btn-icon ml-auto" :aria-label="t('viewers.refresh')" :data-tip="t('viewers.refresh')" @click="load"><Refresh :size="15" /></button>
      </div>
      <div class="mb-4 flex flex-wrap gap-3">
        <select v-if="profiles.length > 1" v-model="profileId" class="input max-w-60">
          <option v-for="p in profiles" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <input v-model="query" class="input max-w-72" :placeholder="t('viewers.search')" />
        <p v-if="profile && !preset" class="hint self-center">{{ t("viewers.notPreset") }}</p>
        <p v-if="profile && !profile.voices.effects" class="hint self-center">{{ t("fx.disabled") }}</p>
      </div>

      <p v-if="!rows.length" class="text-sm text-muted">{{ query ? t("viewers.noMatch") : t("viewers.empty") }}</p>
      <div v-else class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead class="text-left text-xs text-muted">
            <tr>
              <th class="pb-2 font-medium">{{ t("viewers.viewer") }}</th>
              <th class="pb-2 font-medium">{{ t("viewers.voice") }}</th>
              <th class="pb-2 font-medium">{{ t("fx.title") }}<UiTip :text="t('tips.fxColumn')" /></th>
              <th class="pb-2 font-medium">{{ t("viewers.lastSeen") }}</th>
              <th class="pb-2 text-right font-medium">{{ t("viewers.messages") }}</th>
              <th class="pb-2" />
            </tr>
          </thead>
          <tbody class="divide-y divide-line">
            <tr v-for="v in rows" :key="v.user_id" :class="v.muted && 'opacity-60'">
              <td class="py-2 pr-3">
                <span class="font-medium" :style="v.color ? { color: v.color } : undefined">{{ v.display || v.login }}</span>
                <span v-if="v.display.toLowerCase() !== v.login" class="ml-1.5 text-xs text-muted">{{ v.login }}</span>
              </td>
              <td class="py-2 pr-3">
                <span v-if="v.pinned" class="badge badge-accent" :data-tip="t('viewers.pinnedTip')">{{ v.pinned }}</span>
                <select v-else-if="preset && voiceNames.length" class="input h-8 max-w-56 py-0 text-sm" :value="v.role == null ? '' : String(v.role % voiceNames.length)" @change="reassign(v, ($event.target as HTMLSelectElement).value)">
                  <option value="">{{ v.role == null ? t("viewers.notYet") : t("viewers.random") }}</option>
                  <option v-for="(n, i) in voiceNames" :key="i" :value="String(i)">{{ i + 1 }} · {{ n }}</option>
                </select>
                <span v-else class="text-muted">{{ v.role == null ? t("viewers.notYet") : `#${v.role + 1}` }}</span>
              </td>
              <td class="py-2 pr-3">
                <select class="input h-8 max-w-44 py-0 text-sm" data-viewer-fx :disabled="!profile?.voices.effects" :value="v.fx ?? ''" @change="setFx(v, ($event.target as HTMLSelectElement).value)">
                  <option value="">{{ v.voice_fx ? t("fx.own", { fx: fxName(v.voice_fx) }) : t("fx.ownNone") }}</option>
                  <option value="none">{{ t("fx.off") }}</option>
                  <option v-for="e in EFFECTS" :key="e" :value="e">{{ fxName(e) }}</option>
                </select>
              </td>
              <td class="py-2 pr-3 text-muted">{{ ago(v.last_seen, locale) }}</td>
              <td class="py-2 pr-3 text-right tabular-nums">{{ v.messages }}</td>
              <td class="py-2 text-right whitespace-nowrap">
                <button class="btn btn-ghost btn-sm btn-icon" :aria-label="v.muted ? t('viewers.unmute') : t('viewers.mute')" :data-tip="v.muted ? t('viewers.unmute') : t('viewers.mute')" @click="mute(v)">
                  <EyeSlash v-if="v.muted" :size="15" /><Eye v-else :size="15" />
                </button>
                <button class="btn btn-ghost btn-sm btn-icon" :aria-label="t('viewers.forget')" :data-tip="t('viewers.forget')" @click="forget(v)"><Trash :size="15" /></button>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="total > rows.length" class="hint mt-2">{{ t("viewers.more", { n: rows.length, total }) }}</p>
      </div>
    </section>
  </div>
</template>
