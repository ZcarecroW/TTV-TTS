<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { BookOpen } from "../icons";

// A link to the matching section of the documentation (in the UI's
// language), for anything that needs more than a tooltip.
const props = defineProps<{ page: string; anchor?: string; label?: string }>();
const { t, locale } = useI18n();
const url = computed(() => `https://zcarecrow.github.io/TTV-TTS/${locale.value === "de" ? "de/" : ""}${props.page}/${props.anchor ? `#${props.anchor}` : ""}`);

async function open() {
  if ("__TAURI_INTERNALS__" in window) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url.value);
  } else {
    window.open(url.value, "_blank", "noopener");
  }
}
</script>

<template>
  <a :href="url" class="docs-link" @click.prevent="open"><BookOpen :size="14" />{{ label ?? t("common.learnMore") }}</a>
</template>
