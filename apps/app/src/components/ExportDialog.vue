<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiTip from "@ttv-tts/ui/components/UiTip.vue";
import { Spinner, Upload } from "@ttv-tts/ui/icons";
import { openerReveal, packs, presets, TIERS, type PresetDetail } from "../api";
import { run, store, toast } from "../store";

// A pack of a preset's computed voices (or the selected ones) to share.
const props = defineProps<{ presetId: string; keys?: string[] }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

const d = ref<PresetDetail | null>(null);
const langs = ref<string[]>([]);
const count = ref("0");

onMounted(async () => {
  const r = await run("preset", () => presets.get(props.presetId));
  if (!r) return emit("close");
  d.value = r;
  langs.value = Object.keys(r.computed);
});

const selected = computed(() => props.keys ?? []);
const tierOptions = computed(() => {
  const n = d.value?.count ?? 0;
  return [...TIERS.filter((x) => x < n).map((x) => ({ value: String(x), label: String(x) })), { value: "0", label: t("compute.all", { n }) }];
});

/** Voices the pack will hold. */
const voices = computed(() => {
  const p = d.value;
  if (!p) return 0;
  if (selected.value.length) return selected.value.length;
  const n = Number(count.value) || p.count;
  return p.voices.slice(0, n).reduce((s, v) => s + v.langs.filter((l) => langs.value.includes(l)).length, 0);
});
/** ~7 s of 24 kHz speech as FLAC. */
const mb = computed(() => Math.max(0.1, (voices.value * 0.19)).toFixed(voices.value > 50 ? 0 : 1));

function toggle(l: string) {
  langs.value = langs.value.includes(l) ? langs.value.filter((x) => x !== l) : [...langs.value, l];
}

async function save() {
  const p = d.value;
  if (!p) return;
  const keyLangs = selected.value.length ? [...new Set(selected.value.map((k) => k.split(":").pop()!))] : langs.value;
  const r = await run("export", () => packs.export(p.id, keyLangs, Number(count.value), selected.value));
  if (r) {
    toast("ok", t("packs.saved", { n: r.voices, mb: (r.bytes / 1048576).toFixed(1) }), { label: t("packs.show"), run: () => openerReveal(r.path) });
    emit("close");
  }
}
</script>

<template>
  <UiModal docs-page="packs" docs-anchor="export" :title="d ? t('packs.exportTitle', { name: d.name }) : t('common.loading')" width="max-w-lg" @close="emit('close')">
    <div v-if="d" class="space-y-5">
      <p class="muted text-sm">{{ t("packs.exportIntro") }}</p>
      <template v-if="!selected.length">
        <div>
          <div class="label">{{ t("compute.languages") }}</div>
          <div class="flex flex-wrap gap-1.5">
            <button v-for="(n, l) in d.computed" :key="l" type="button" class="chip" :class="langs.includes(String(l)) && 'chip-on'" @click="toggle(String(l))">
              {{ t(`lang.${l}`) }}<span class="opacity-70">{{ n }}</span>
            </button>
          </div>
        </div>
        <UiSegmented v-model="count" :options="tierOptions" :label="t('compute.voices')" :tip="t('tips.packVoices')" />
      </template>
      <p v-else class="text-sm">{{ t("packs.selected", { n: selected.length }) }}</p>
      <div class="rounded-xl border border-line bg-panel-2/40 p-4 text-sm">
        <span class="text-base font-semibold">{{ t("packs.voices", voices) }}</span>
        <span class="ml-2 text-muted">{{ t("packs.size", { mb }) }}</span>
        <UiTip :text="t('tips.packContents')" />
      </div>
    </div>
    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.cancel") }}</button>
      <button class="btn btn-primary" :disabled="!voices || store.busy === 'export'" @click="save">
        <Spinner v-if="store.busy === 'export'" :size="15" class="spin" /><Upload v-else :size="15" />{{ t("packs.save") }}
      </button>
    </template>
  </UiModal>
</template>
