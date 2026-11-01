<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import { Lock, Spinner } from "@ttv-tts/ui/icons";
import { closePairing, openPairing, store } from "../store";

const { t } = useI18n();
const left = ref(store.pairing?.secs_left ?? 0);
let timer = 0;
onMounted(() => {
  timer = window.setInterval(() => (left.value = Math.max(0, left.value - 1)), 1000);
});
onBeforeUnmount(() => clearInterval(timer));

const digits = computed(() => (store.pairing?.code ?? "").split(""));
const time = computed(() => `${Math.floor(left.value / 60)}:${String(left.value % 60).padStart(2, "0")}`);

async function renew() {
  await openPairing();
  left.value = store.pairing?.secs_left ?? 0;
}
</script>

<template>
  <UiModal :title="t('connect.dialogTitle')" width="max-w-md" @close="closePairing()">
    <p class="text-sm">{{ t("connect.dialogText") }}</p>
    <div class="my-5 flex justify-center gap-2" :class="left === 0 && 'opacity-30'">
      <span
        v-for="(d, i) in digits"
        :key="i"
        class="grid h-16 w-12 place-items-center rounded-xl border border-line-strong bg-panel-2 font-mono text-3xl font-semibold"
        :class="i === 2 && 'mr-2'"
      >{{ d }}</span>
    </div>
    <p v-if="left > 0" class="flex items-center justify-center gap-2 text-sm text-muted">
      <Spinner :size="15" class="spin" />{{ t("connect.dialogWaiting") }} · {{ t("connect.dialogExpires", { time }) }}
    </p>
    <div v-else class="text-center">
      <p class="text-sm text-muted">{{ t("connect.dialogExpired") }}</p>
      <button class="btn btn-primary btn-sm mt-2" @click="renew">{{ t("connect.dialogNew") }}</button>
    </div>
    <div class="notice notice-info mt-5">
      <Lock :size="18" />
      <div>
        <p class="text-xs">{{ t("connect.dialogCompare") }}</p>
        <code class="font-mono text-base font-semibold">{{ store.ov?.fingerprint_short }}</code>
      </div>
    </div>
  </UiModal>
</template>
