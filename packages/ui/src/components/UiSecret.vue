<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { Check, Copy, Eye, EyeSlash } from "../icons";
import { copyText } from "../clipboard";

// A secret (API key, token) shown masked, with reveal and copy buttons.
const props = defineProps<{ value: string | null | undefined; empty?: string }>();
const { t } = useI18n();
const shown = ref(false);
const copied = ref(false);

async function copy() {
  if (!props.value) return;
  if (await copyText(props.value)) {
    copied.value = true;
    setTimeout(() => (copied.value = false), 1500);
  }
}
</script>

<template>
  <div class="flex min-w-0 items-center gap-1.5">
    <code class="input mono flex min-w-0 flex-1 items-center overflow-hidden text-ellipsis whitespace-nowrap" :class="!value && 'text-muted'">
      {{ value ? (shown ? value : "•".repeat(Math.min(value.length, 32))) : (empty ?? "—") }}
    </code>
    <button
      v-if="value"
      type="button"
      class="btn btn-icon"
      :data-tip="shown ? t('common.hide') : t('common.show')"
      :aria-label="shown ? t('common.hide') : t('common.show')"
      @click="shown = !shown"
    >
      <EyeSlash v-if="shown" :size="16" /><Eye v-else :size="16" />
    </button>
    <button v-if="value" type="button" class="btn btn-icon" :data-tip="t('common.copy')" :aria-label="t('common.copy')" @click="copy">
      <Check v-if="copied" :size="16" class="text-ok" /><Copy v-else :size="16" />
    </button>
  </div>
</template>
