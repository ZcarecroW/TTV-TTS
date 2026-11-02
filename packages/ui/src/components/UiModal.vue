<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { X } from "../icons";
import UiDocs from "./UiDocs.vue";

const props = withDefaults(defineProps<{ title?: string; width?: string; closable?: boolean; docsPage?: string; docsAnchor?: string }>(), { width: "max-w-lg", closable: true });
const emit = defineEmits<{ close: [] }>();

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && props.closable) emit("close");
}
onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4" @click.self="closable && emit('close')">
      <div class="card flex max-h-full w-full flex-col overflow-hidden" :class="width" role="dialog" aria-modal="true">
        <header v-if="title || closable" class="flex items-center justify-between border-b border-line px-5 py-3">
          <h2 class="text-base font-semibold">{{ title }}</h2>
          <UiDocs v-if="docsPage" class="ml-auto mr-2" :page="docsPage" :anchor="docsAnchor" />
          <button v-if="closable" class="btn btn-ghost btn-sm btn-icon" :aria-label="'Close'" @click="emit('close')"><X :size="16" /></button>
        </header>
        <div class="min-h-0 flex-1 overflow-y-auto p-5">
          <slot />
        </div>
        <footer v-if="$slots.footer" class="flex justify-end gap-2 border-t border-line px-5 py-3">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>
