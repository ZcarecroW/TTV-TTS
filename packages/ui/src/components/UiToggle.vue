<script setup lang="ts">
import UiTip from "./UiTip.vue";
defineProps<{ label?: string; hint?: string; tip?: string; disabled?: boolean }>();
const model = defineModel<boolean>({ required: true });
</script>

<template>
  <label class="flex items-start gap-3" :class="disabled ? 'cursor-not-allowed opacity-60' : 'cursor-pointer'">
    <button
      type="button"
      role="switch"
      :aria-checked="model"
      :disabled="disabled"
      class="relative mt-0.5 inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors"
      :class="model ? 'bg-accent' : 'bg-line-strong'"
      @click="model = !model"
    >
      <span class="inline-block h-4 w-4 rounded-full bg-white shadow transition-transform" :class="model ? 'translate-x-4.5' : 'translate-x-0.5'" />
    </button>
    <span v-if="label || hint || $slots.default" class="min-w-0">
      <span class="block text-sm font-medium text-fg"><slot>{{ label }}</slot><UiTip v-if="tip" :text="tip" /></span>
      <span v-if="hint" class="hint block">{{ hint }}</span>
    </span>
  </label>
</template>
