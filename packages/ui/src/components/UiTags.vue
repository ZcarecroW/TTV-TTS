<script setup lang="ts">
import { ref } from "vue";
import { X } from "../icons";

// A list of short strings as removable chips; Enter or comma adds one.
const props = defineProps<{ placeholder?: string; lower?: boolean; mono?: boolean }>();
const model = defineModel<string[]>({ required: true });
const draft = ref("");

function add() {
  const parts = draft.value.split(",").map((s) => (props.lower ? s.trim().toLowerCase() : s.trim())).filter(Boolean);
  if (parts.length) model.value = [...model.value, ...parts.filter((p) => !model.value.includes(p))];
  draft.value = "";
}
function remove(i: number) {
  model.value = model.value.filter((_, j) => j !== i);
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Enter" || e.key === ",") {
    e.preventDefault();
    add();
  } else if (e.key === "Backspace" && !draft.value && model.value.length) {
    remove(model.value.length - 1);
  }
}
</script>

<template>
  <div class="input flex h-auto min-h-[2.15rem] flex-wrap items-center gap-1 py-1" @click="($refs.inp as HTMLInputElement).focus()">
    <span v-for="(v, i) in model" :key="v" class="badge badge-accent gap-1 normal-case" :class="mono && 'font-mono'">
      {{ v }}
      <button type="button" class="-mr-1 rounded-full opacity-70 hover:opacity-100" :aria-label="`remove ${v}`" @click.stop="remove(i)"><X :size="12" /></button>
    </span>
    <input ref="inp" v-model="draft" class="min-w-24 flex-1 bg-transparent text-sm outline-none" :placeholder="placeholder" @keydown="onKey" @blur="add" />
  </div>
</template>
