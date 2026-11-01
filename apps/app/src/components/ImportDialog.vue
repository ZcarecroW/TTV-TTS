<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import { Download, Spinner } from "@ttv-tts/ui/icons";
import { packs, type PackInfo } from "../api";
import { host, loadEngine, loadPresets, run, store, toast } from "../store";

// Shows what a voice pack holds and adds it.
const props = defineProps<{ path: string }>();
const emit = defineEmits<{ close: [] }>();
const { t, locale } = useI18n();

const info = ref<PackInfo | null>(null);
onMounted(async () => {
  const r = await run("packInfo", () => packs.info(props.path));
  if (!r) return emit("close");
  info.value = r;
});

async function add() {
  const r = await run("import", () => packs.import(props.path));
  if (r) {
    toast("ok", r.job.total ? t("packs.importing", { n: r.job.total }) : t("packs.nothingNew"));
    await Promise.all([loadEngine(), loadPresets()]);
    emit("close");
  }
}
</script>

<template>
  <UiModal :title="info?.title || t('packs.importTitle')" width="max-w-lg" @close="emit('close')">
    <div v-if="info" class="space-y-4 text-sm">
      <p class="muted">{{ t("packs.importIntro") }}</p>
      <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5">
        <dt class="text-muted">{{ t("packs.voicesLabel") }}</dt>
        <dd>
          {{ t("packs.voices", info.voices) }}
          <span v-if="info.voices !== info.new_voices" class="text-muted"> · {{ t("packs.have", { n: info.voices - info.new_voices }) }}</span>
        </dd>
        <dt class="text-muted">{{ t("compute.languages") }}</dt>
        <dd>{{ info.languages.map((l) => t(`lang.${l}`)).join(", ") }}</dd>
        <dt class="text-muted">{{ t("packs.presetsLabel") }}</dt>
        <dd>
          <span v-for="p in info.presets" :key="p.id" class="mr-2 inline-flex items-center gap-1">
            {{ p.name }}<span class="badge" :class="p.status === 'new' ? 'badge-accent' : ''">{{ t(`packs.status.${p.status}`) }}</span>
          </span>
        </dd>
        <dt class="text-muted">{{ t("packs.made") }}</dt>
        <dd>{{ info.created ? new Date(info.created * 1000).toLocaleDateString(locale) : "–" }} · TTV-TTS {{ info.app }} · {{ (info.bytes / 1048576).toFixed(1) }} MB</dd>
      </dl>
      <p v-if="!host?.connected" class="notice notice-warn">{{ t("presets.noHost") }}</p>
    </div>
    <p v-else class="text-sm text-muted"><Spinner :size="14" class="spin" /> {{ t("common.loading") }}</p>
    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.cancel") }}</button>
      <button class="btn btn-primary" :disabled="!info || !host?.connected || store.busy === 'import'" @click="add">
        <Spinner v-if="store.busy === 'import'" :size="15" class="spin" /><Download v-else :size="15" />{{ info && info.new_voices ? t("packs.add", info.new_voices) : t("packs.addPresets") }}
      </button>
    </template>
  </UiModal>
</template>
