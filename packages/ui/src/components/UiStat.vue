<script setup lang="ts">
import { computed } from "vue";

// A metric tile: label, value, optional bar (0–1) that turns amber/red when high.
const props = defineProps<{ label: string; value: string; sub?: string; ratio?: number | null; tip?: string }>();
const level = computed(() => {
  const r = props.ratio ?? 0;
  return r > 0.92 ? "bg-err" : r > 0.75 ? "bg-warn" : "bg-accent";
});
</script>

<template>
  <div class="rounded-xl border border-line bg-panel-2 p-3" :data-tip="tip">
    <div class="flex items-center gap-1.5 text-xs font-medium text-muted"><slot name="icon" />{{ label }}</div>
    <div class="mt-1 text-lg font-semibold tabular-nums leading-tight">{{ value }}</div>
    <div v-if="sub" class="truncate text-xs text-muted">{{ sub }}</div>
    <div v-if="ratio != null" class="mt-2 h-1.5 overflow-hidden rounded-full bg-panel-3">
      <div class="h-full rounded-full transition-[width] duration-500" :class="level" :style="{ width: `${Math.round(Math.min(1, Math.max(0, ratio)) * 100)}%` }" />
    </div>
  </div>
</template>
