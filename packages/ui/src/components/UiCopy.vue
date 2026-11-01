<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { Check, Copy } from "../icons";
import { copyText } from "../clipboard";

// A small copy button for a value shown next to it.
const props = defineProps<{ value: string }>();
const { t } = useI18n();
const copied = ref(false);

async function copy() {
  if (await copyText(props.value)) {
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  }
}
</script>

<template>
  <button type="button" class="btn btn-ghost btn-sm btn-icon" :data-tip="t('common.copy')" :aria-label="t('common.copy')" @click.stop="copy">
    <Check v-if="copied" :size="15" class="text-ok" /><Copy v-else :size="15" />
  </button>
</template>
