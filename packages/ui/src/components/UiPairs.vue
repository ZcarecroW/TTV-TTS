<script setup lang="ts">
import { computed, ref } from "vue";
import { Plus, Trash } from "../icons";

// Edits a string -> string map as rows (e.g. pronunciations, replacements).
const props = defineProps<{ keyLabel: string; valueLabel: string; addLabel: string; lowerKeys?: boolean }>();
const model = defineModel<Record<string, string>>({ required: true });
const k = ref("");
const v = ref("");
const rows = computed(() => Object.entries(model.value).sort(([a], [b]) => a.localeCompare(b)));

function add() {
  const key = props.lowerKeys ? k.value.trim().toLowerCase() : k.value.trim();
  if (!key) return;
  model.value = { ...model.value, [key]: v.value.trim() };
  k.value = "";
  v.value = "";
}
function remove(key: string) {
  const next = { ...model.value };
  delete next[key];
  model.value = next;
}
function edit(key: string, value: string) {
  model.value = { ...model.value, [key]: value };
}
</script>

<template>
  <div class="space-y-1.5">
    <div v-for="[key, value] in rows" :key="key" class="flex items-center gap-1.5">
      <code class="input mono flex w-2/5 items-center truncate">{{ key }}</code>
      <input class="input flex-1" :value="value" @change="edit(key, ($event.target as HTMLInputElement).value)" />
      <button type="button" class="btn btn-ghost btn-sm btn-icon" :aria-label="`remove ${key}`" @click="remove(key)"><Trash :size="15" /></button>
    </div>
    <div class="flex items-center gap-1.5">
      <input v-model="k" class="input mono w-2/5" :placeholder="keyLabel" @keydown.enter="add" />
      <input v-model="v" class="input flex-1" :placeholder="valueLabel" @keydown.enter="add" />
      <button type="button" class="btn btn-sm btn-icon" :disabled="!k.trim()" :aria-label="addLabel" :data-tip="addLabel" @click="add"><Plus :size="15" /></button>
    </div>
  </div>
</template>
