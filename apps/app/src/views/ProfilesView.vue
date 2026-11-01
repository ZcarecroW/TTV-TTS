<script setup lang="ts">
import { computed, onMounted, ref, toRaw, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import UiPairs from "@ttv-tts/ui/components/UiPairs.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiTags from "@ttv-tts/ui/components/UiTags.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import { Devices, Gear, Globe, Lock, Plus, Sparkle, Trash, Waveform } from "@ttv-tts/ui/icons";
import { api, reader, TIERS, type Profile, type Role, type Voice } from "../api";
import { run, store } from "../store";

const { t } = useI18n();
const profiles = computed(() => store.reader!.store.profiles);
const selectedId = ref(profiles.value[0]?.id ?? "");
const p = ref<Profile | null>(null);
const voices = ref<Voice[]>([]);
const replLang = ref("all");

const tryText = ref("");
const tried = ref("");
async function tryIt() {
  const pr = p.value;
  if (!pr || !tryText.value.trim()) return;
  const into = pr.translate.into || pr.language.default;
  const r = await run("translate", () => api.tryTranslate(tryText.value, into, pr.translate.provider));
  if (r) tried.value = `${r.text} · ${t("profiles.translateTook", { s: (r.ms / 1000).toFixed(1) })}`;
}
const LANGS = ["de", "en", "ru", "uk", "fr", "es", "it", "pt", "nl", "pl", "tr", "ja", "zh", "ko"];

// Preset mode: which preset, how many of its voices.
const chosenPreset = computed(() => store.presets.find((x) => x.id === p.value?.voices.preset));
const tierOptions = computed(() => {
  const n = chosenPreset.value?.count ?? 0;
  return [...TIERS.filter((x) => x < n), 0];
});
const presetHint = computed(() => {
  const c = chosenPreset.value;
  if (!c || !p.value) return "";
  const langs = Object.entries(c.computed).map(([l, n]) => `${l} ${n}`).join(" · ");
  return langs ? t("profiles.presetComputed", { langs }) : t("profiles.presetNothing");
});
const ROLES: Role[] = ["broadcaster", "moderator", "vip", "subscriber"];

// Store profiles are reactive proxies: unwrap before copying.
const clone = <T,>(x: T): T => structuredClone(toRaw(x));

function select(id: string) {
  selectedId.value = id;
  const found = profiles.value.find((x) => x.id === id);
  p.value = found ? clone(found) : null;
}
onMounted(async () => {
  select(selectedId.value);
  voices.value = (await api.voices().catch(() => [])) ?? [];
});

// Auto-save: 600 ms after the last change.
let timer = 0;
let loading = false;
watch(
  p,
  (val) => {
    if (!val || loading) return;
    clearTimeout(timer);
    timer = window.setTimeout(async () => {
      const v = await run("save", () => reader.saveProfile(val));
      if (v) store.reader!.store = v.store;
    }, 600);
  },
  { deep: true },
);
watch(selectedId, () => {
  loading = true;
  select(selectedId.value);
  setTimeout(() => (loading = false));
});

async function create() {
  const base = clone(profiles.value[0]!);
  const fresh: Profile = { ...base, id: "", name: t("profiles.new"), channels: [], muted: [] };
  const v = await run("save", () => reader.saveProfile(fresh));
  if (v) {
    store.reader!.store = v.store;
    selectedId.value = v.store.profiles.at(-1)!.id;
  }
}
async function remove() {
  if (!p.value || !confirm(t("profiles.deleteConfirm", { name: p.value.name }))) return;
  const v = await run("save", () => reader.deleteProfile(p.value!.id));
  if (v) {
    store.reader!.store = v.store;
    selectedId.value = v.store.profiles[0]?.id ?? "";
  }
}

function toggleRole(list: Role[], role: Role) {
  const i = list.indexOf(role);
  if (i >= 0) list.splice(i, 1);
  else list.push(role);
}

const replMap = computed({
  get: () => p.value?.text.replacements[replLang.value] ?? {},
  set: (m: Record<string, string>) => {
    if (p.value) p.value.text.replacements = { ...p.value.text.replacements, [replLang.value]: m };
  },
});
const defaultVoice = computed({
  get: () => p.value?.voices.default_voice ?? "",
  set: (v: string) => {
    if (p.value) p.value.voices.default_voice = v || null;
  },
});
</script>

<template>
  <div class="space-y-5">
    <div class="flex flex-wrap items-center gap-2">
      <button
        v-for="x in profiles"
        :key="x.id"
        type="button"
        class="btn"
        :class="x.id === selectedId ? 'btn-primary' : ''"
        @click="selectedId = x.id"
      >
        <span class="dot" :class="store.reader!.store.active.includes(x.id) ? 'dot-ok' : ''" />{{ x.name }}
      </button>
      <button class="btn btn-ghost" @click="create"><Plus :size="16" />{{ t("profiles.new") }}</button>
      <span class="ml-auto text-xs text-muted">{{ t("profiles.autosave") }}</span>
    </div>

    <template v-if="p">
      <section class="card p-5">
        <div class="card-head"><Devices />{{ t("profiles.basics") }}<UiTip :text="t('tips.profiles')" />
          <button v-if="profiles.length > 1" class="btn btn-danger btn-sm ml-auto" @click="remove"><Trash :size="15" />{{ t("profiles.delete") }}</button>
        </div>
        <div class="grid gap-4 md:grid-cols-[1fr_2fr]">
          <UiField :label="t('profiles.name')"><input v-model="p.name" class="input" /></UiField>
          <UiField :label="t('profiles.channels')" :tip="t('tips.channels')">
            <UiTags v-model="p.channels" lower mono :placeholder="t('profiles.channelsPlaceholder')" />
          </UiField>
        </div>
      </section>

      <div class="grid gap-5 lg:grid-cols-2">
        <section class="card p-5">
          <div class="card-head"><Waveform />{{ t("profiles.reading") }}</div>
          <div class="space-y-4">
            <UiSegmented
              v-model="p.reading.mode"
              :label="t('profiles.mode')"
              :tip="t('tips.readMode')"
              :options="[
                { value: 'all', label: t('profiles.modeAll') },
                { value: 'command', label: t('profiles.modeCommand') },
                { value: 'privileged', label: t('profiles.modePrivileged') },
              ]"
            />
            <UiField v-if="p.reading.mode === 'command'" :label="t('profiles.command')"><input v-model="p.reading.command" class="input mono max-w-40" /></UiField>
            <div v-if="p.reading.mode === 'privileged'">
              <span class="label">{{ t("profiles.roles") }}</span>
              <div class="flex flex-wrap gap-1.5">
                <button v-for="r in ROLES" :key="r" type="button" class="btn btn-sm" :class="p.reading.allowed_roles.includes(r) && 'btn-primary'" @click="toggleRole(p.reading.allowed_roles, r)">{{ t(`profiles.roleNames.${r}`) }}</button>
              </div>
            </div>
            <UiField :label="t('profiles.rewardId')" :tip="t('tips.rewardId')"><input v-model="p.reading.reward_id" class="input mono" :placeholder="t('profiles.rewardIdPlaceholder')" /></UiField>
            <UiField :label="t('profiles.ignoreUsers')" :tip="t('tips.ignoreUsers')"><UiTags v-model="p.reading.ignore_users" lower mono /></UiField>
            <UiField :label="t('profiles.ignorePrefixes')"><UiTags v-model="p.reading.ignore_prefixes" mono /></UiField>
            <UiToggle v-model="p.reading.ignore_broadcaster" :label="t('profiles.ignoreBroadcaster')" />
            <UiToggle v-model="p.reading.read_sub_messages" :label="t('profiles.readSubs')" />
          </div>
        </section>

        <section class="card p-5">
          <div class="card-head"><Gear />{{ t("profiles.limitsTitle") }}</div>
          <div class="grid grid-cols-2 gap-4">
            <UiField :label="t('profiles.maxChars')" :tip="t('tips.maxChars')"><input v-model.number="p.reading.max_chars" type="number" min="20" max="2000" class="input" /></UiField>
            <UiField :label="t('profiles.maxSeconds')"><input v-model.number="p.reading.max_seconds" type="number" min="5" max="300" class="input" /></UiField>
            <UiField :label="t('profiles.cooldown')" :tip="t('tips.cooldown')"><input v-model.number="p.reading.user_cooldown_secs" type="number" min="0" class="input" /></UiField>
            <UiField :label="t('profiles.maxQueue')" :tip="t('tips.maxQueue')"><input v-model.number="p.reading.max_queue" type="number" min="1" max="200" class="input" /></UiField>
          </div>
          <div class="card-head mt-6"><Devices />{{ t("profiles.names") }}</div>
          <div class="space-y-4">
            <UiSegmented
              v-model="p.names.announce"
              :label="t('profiles.announce')"
              :tip="t('tips.announce')"
              :options="[
                { value: 'on_change', label: t('profiles.announceOnChange') },
                { value: 'always', label: t('profiles.announceAlways') },
                { value: 'never', label: t('profiles.announceNever') },
              ]"
            />
            <UiField v-if="p.names.announce === 'on_change'" :label="t('profiles.resetSecs')"><input v-model.number="p.names.reset_secs" type="number" min="0" class="input max-w-28" /></UiField>
            <UiToggle v-model="p.names.cleanup" :label="t('profiles.cleanup')" />
            <UiField :label="t('profiles.aliases')" :tip="t('tips.aliases')">
              <UiPairs v-model="p.names.aliases" lower-keys :key-label="t('profiles.aliasKey')" :value-label="t('profiles.aliasValue')" :add-label="t('profiles.add')" />
            </UiField>
          </div>
        </section>

        <section class="card p-5">
          <div class="card-head"><Globe />{{ t("profiles.language") }}</div>
          <div class="grid gap-4 sm:grid-cols-2">
            <UiField :label="t('profiles.defaultLang')" :tip="t('tips.defaultLang')">
              <select v-model="p.language.default" class="input"><option v-for="l in LANGS" :key="l" :value="l">{{ l }}</option></select>
            </UiField>
            <UiField :label="t('profiles.unsupported')" :tip="t('tips.unsupported')">
              <select v-model="p.language.unsupported" class="input">
                <option value="auto">{{ t("profiles.unsupportedAuto") }}</option>
                <option v-for="l in LANGS" :key="l" :value="l">{{ l }}</option>
              </select>
            </UiField>
            <UiField :label="t('profiles.remember')"><input v-model.number="p.language.remember_minutes" type="number" min="0" class="input max-w-28" /></UiField>
          </div>
        </section>

        <section class="card p-5">
          <div class="card-head"><Sparkle />{{ t("profiles.emotes") }}</div>
          <div class="space-y-4">
            <UiSegmented
              v-model="p.emotes.mode"
              :label="t('profiles.emoteMode')"
              :tip="t('tips.emoteMode')"
              :options="[
                { value: 'once', label: t('profiles.emoteOnce') },
                { value: 'remove', label: t('profiles.emoteRemove') },
                { value: 'keep', label: t('profiles.emoteKeep') },
              ]"
            />
            <div class="flex flex-wrap gap-4">
              <UiToggle v-model="p.emotes.bttv" label="BTTV" />
              <UiToggle v-model="p.emotes.ffz" label="FFZ" />
              <UiToggle v-model="p.emotes.seventv" label="7TV" />
            </div>
            <UiToggle v-model="p.emotes.split_words" :label="t('profiles.splitWords')" />
            <UiToggle v-model="p.emotes.skip_emote_only" :label="t('profiles.skipEmoteOnly')" />
            <UiField :label="t('profiles.emoteAliases')" :tip="t('tips.aliases')">
              <UiPairs v-model="p.emotes.aliases" :key-label="t('profiles.aliasEmote')" :value-label="t('profiles.aliasValue')" :add-label="t('profiles.add')" />
            </UiField>
            <UiSegmented
              v-model="p.emojis.mode"
              :label="t('profiles.emojiMode')"
              :options="[
                { value: 'describe', label: t('profiles.emojiDescribe') },
                { value: 'remove', label: t('profiles.emojiRemove') },
                { value: 'keep', label: t('profiles.emojiKeep') },
              ]"
            />
          </div>
        </section>

        <section class="card p-5">
          <div class="card-head"><Lock />{{ t("profiles.text") }}</div>
          <div class="space-y-4">
            <UiSegmented
              v-model="p.text.links"
              :label="t('profiles.links')"
              :options="[
                { value: 'replace', label: t('profiles.linkReplace') },
                { value: 'remove', label: t('profiles.linkRemove') },
                { value: 'skip', label: t('profiles.linkSkip') },
              ]"
            />
            <UiSegmented
              v-model="p.text.mentions"
              :label="t('profiles.mentions')"
              :options="[
                { value: 'name', label: t('profiles.mentionName') },
                { value: 'remove', label: t('profiles.mentionRemove') },
              ]"
            />
            <UiToggle v-model="p.text.collapse_repeats" :label="t('profiles.repeats')" />
            <UiToggle v-model="p.text.fix_shouting" :label="t('profiles.shouting')" />
            <UiField :label="t('profiles.blocked')" :tip="t('tips.blocked')"><UiTags v-model="p.text.blocked_words" /></UiField>
            <UiSegmented
              v-if="p.text.blocked_words.length"
              v-model="p.text.blocked_mode"
              :label="t('profiles.blockedMode')"
              :options="[
                { value: 'skip', label: t('profiles.blockedSkip') },
                { value: 'beep', label: t('profiles.blockedBeep') },
              ]"
            />
            <div>
              <span class="label">{{ t("profiles.replacements") }}<UiTip :text="t('tips.replacements')" />
                <span class="ml-2 font-normal text-muted">{{ t("profiles.replacementsFor") }}</span>
                <select v-model="replLang" class="input ml-1 h-7 w-auto py-0 text-xs">
                  <option value="all">{{ t("profiles.replacementsAll") }}</option>
                  <option v-for="l in LANGS" :key="l" :value="l">{{ l }}</option>
                </select>
              </span>
              <UiPairs v-model="replMap" lower-keys :key-label="t('profiles.replaceFrom')" :value-label="t('profiles.replaceTo')" :add-label="t('profiles.add')" />
            </div>
          </div>
        </section>

        <section class="card p-5" data-translate>
          <div class="card-head"><Globe />{{ t("profiles.translateTitle") }}<UiTip :text="t('tips.translate')" /></div>
          <div class="space-y-4">
            <UiToggle v-model="p.translate.enabled" :label="t('profiles.translate')" />
            <template v-if="p.translate.enabled">
              <div class="grid grid-cols-2 gap-3">
                <UiField :label="t('profiles.translateInto')">
                  <select v-model="p.translate.into" class="input">
                    <option value="">{{ t("profiles.translateIntoDefault", { lang: p.language.default }) }}</option>
                    <option v-for="l in LANGS" :key="l" :value="l">{{ l }}</option>
                  </select>
                </UiField>
                <UiField :label="t('profiles.translateProvider')" :tip="t('tips.translateProvider')">
                  <select v-model="p.translate.provider" class="input">
                    <option value="mymemory">{{ t("profiles.providerMyMemory") }}</option>
                    <option value="deepl">{{ t("profiles.providerDeepl") }}</option>
                    <option value="vcp">{{ t("profiles.providerVcp") }}</option>
                  </select>
                </UiField>
              </div>
              <UiField :label="t('profiles.translateMine')" :tip="t('tips.translateMine')"><UiTags v-model="p.translate.mine" lower mono /></UiField>
              <UiSegmented
                v-model="p.translate.read"
                :label="t('profiles.translateRead')"
                :options="[
                  { value: 'translation', label: t('profiles.readTranslation') },
                  { value: 'both', label: t('profiles.readBoth') },
                  { value: 'show', label: t('profiles.readShow') },
                ]"
              />
              <p v-if="p.translate.provider === 'deepl' && !store.app?.deepl_key" class="notice notice-warn text-xs">{{ t("profiles.deeplMissing") }}</p>
              <div class="flex gap-2">
                <input v-model="tryText" class="input" :placeholder="t('profiles.translateTry')" @keydown.enter="tryIt" />
                <button class="btn" :disabled="!tryText.trim() || store.busy === 'translate'" @click="tryIt">{{ t("profiles.translateTryButton") }}</button>
              </div>
              <p v-if="tried" class="text-sm" data-tried>{{ tried }}</p>
            </template>
          </div>
        </section>

        <section class="card p-5">
          <div class="card-head"><Gear />{{ t("profiles.mod") }}<UiTip :text="t('tips.mod')" /></div>
          <div class="space-y-4">
            <UiToggle v-model="p.mod_commands.enabled" :label="t('profiles.modEnabled')" />
            <template v-if="p.mod_commands.enabled">
              <div>
                <span class="label">{{ t("profiles.modRoles") }}</span>
                <div class="flex flex-wrap gap-1.5">
                  <button v-for="r in ROLES" :key="r" type="button" class="btn btn-sm" :class="p.mod_commands.roles.includes(r) && 'btn-primary'" @click="toggleRole(p.mod_commands.roles, r)">{{ t(`profiles.roleNames.${r}`) }}</button>
                </div>
              </div>
              <div class="grid grid-cols-2 gap-2">
                <UiField v-for="c in (['skip', 'clear', 'pause', 'resume', 'volume', 'mute', 'unmute'] as const)" :key="c" :label="t(`profiles.cmd.${c}`)">
                  <input v-model="p.mod_commands[c]" class="input mono" />
                </UiField>
              </div>
            </template>
            <UiToggle v-model="p.mod_commands.skip_on_timeout" :label="t('profiles.modSkipOnTimeout')" />
          </div>
        </section>

        <section class="card p-5 lg:col-span-2">
          <div class="card-head"><Waveform />{{ t("profiles.voices") }}</div>
          <div class="grid gap-4 lg:grid-cols-2">
            <div class="space-y-4">
              <UiSegmented
                v-model="p.voices.mode"
                :label="t('profiles.voiceMode')"
                :tip="t('tips.voiceMode')"
                :options="[
                  { value: 'preset', label: t('profiles.vmPreset') },
                  { value: 'per_user_language', label: t('profiles.vmPerUserLanguage') },
                  { value: 'per_user', label: t('profiles.vmPerUser') },
                  { value: 'per_language', label: t('profiles.vmPerLanguage') },
                  { value: 'single', label: t('profiles.vmSingle') },
                ]"
              />
              <template v-if="p.voices.mode === 'preset'">
                <div class="grid grid-cols-[2fr_1fr] gap-3">
                  <UiField :label="t('profiles.preset')" :tip="t('tips.profilePreset')">
                    <select v-model="p.voices.preset" class="input">
                      <option v-for="x in store.presets" :key="x.id" :value="x.id">{{ x.name }}</option>
                    </select>
                  </UiField>
                  <UiField :label="t('profiles.presetVoices')" :tip="t('tips.presetVoices')">
                    <select v-model.number="p.voices.preset_voices" class="input">
                      <option v-for="n in tierOptions" :key="n" :value="n">{{ n === 0 ? t("profiles.allVoices", { n: chosenPreset?.count ?? 0 }) : n }}</option>
                    </select>
                  </UiField>
                </div>
                <p v-if="chosenPreset" class="hint -mt-2">{{ presetHint }}</p>
                <UiField :label="t('profiles.fallbackLang')" :tip="t('tips.presetFallback')">
                  <select v-model="p.voices.fallback_language" class="input"><option v-for="l in LANGS" :key="l" :value="l">{{ l }}</option></select>
                </UiField>
              </template>
              <div v-if="p.voices.mode === 'per_user_language'" class="grid grid-cols-2 gap-3">
                <UiField :label="t('profiles.prefix')" :tip="t('tips.prefix')"><input v-model="p.voices.prefix" class="input mono" /></UiField>
                <UiField :label="t('profiles.fallbackLang')">
                  <select v-model="p.voices.fallback_language" class="input"><option v-for="l in LANGS" :key="l" :value="l">{{ l }}</option></select>
                </UiField>
              </div>
              <UiField :label="t('profiles.defaultVoice')">
                <select v-model="defaultVoice" class="input">
                  <option value="">{{ t("profiles.defaultVoiceNone") }}</option>
                  <option v-for="v in voices" :key="v.id" :value="v.id">{{ v.name }}</option>
                </select>
              </UiField>
              <UiToggle v-model="p.voices.effects" :label="t('profiles.effects')" :tip="t('tips.voiceEffects')" />
              <UiToggle v-model="p.voices.emotions" :label="t('profiles.emotions')" :tip="t('tips.emotions')" />
              <UiSegmented
                v-if="p.voices.emotions"
                v-model="p.voices.emotion_strength"
                :label="t('profiles.emotionStrength')"
                :tip="t('tips.emotionStrength')"
                :options="[
                  { value: 'mild', label: t('emotions.levels.mild') },
                  { value: 'normal', label: t('emotions.levels.normal') },
                  { value: 'strong', label: t('emotions.levels.strong') },
                ]"
              />
            </div>
            <div class="space-y-4">
              <UiField :label="t('profiles.userVoices')" :tip="t('tips.userVoices')">
                <UiPairs v-model="p.voices.users" lower-keys :key-label="t('profiles.aliasKey')" :value-label="t('voices.voice')" :add-label="t('profiles.add')" />
              </UiField>
              <UiField :label="t('profiles.muted')"><UiTags v-model="p.muted" lower mono /></UiField>
            </div>
          </div>
        </section>
      </div>
    </template>
  </div>
</template>
