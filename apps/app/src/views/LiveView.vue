<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { CircleInfo, Server, Waveform } from "@ttv-tts/ui/icons";
import { host, store } from "../store";

const { t } = useI18n();
</script>

<template>
  <div class="space-y-5">
    <section class="card p-5">
      <div class="card-head"><CircleInfo />{{ t("live.comingTitle") }}</div>
      <p class="text-sm">{{ t("live.comingText") }}</p>
      <div class="mt-4 flex flex-wrap gap-2">
        <button class="btn" @click="store.view = 'host'"><Server :size="16" />{{ host?.kind === "none" ? t("host.setupTitle") : t("nav.host") }}</button>
        <button class="btn btn-primary" @click="store.view = 'voices'"><Waveform :size="16" />{{ t("voices.tryTitle") }}</button>
      </div>
    </section>
  </div>
</template>
