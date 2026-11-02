<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { ago } from "../format";
import { Lock, Spinner, Warning } from "../icons";
import type { CertStatus } from "../friends";

// The HTTPS certificate of OBS links from the internet, in one line: valid,
// being fetched, failed (with the next try), or not needed yet.
const props = defineProps<{ status: CertStatus | null | undefined; on: boolean }>();
const { t, locale } = useI18n();
const AGREEMENT = "https://letsencrypt.org/repository/";

const now = () => Date.now() / 1000;
const date = (s: number | null) => (s ? new Date(s * 1000).toLocaleDateString(locale.value) : "–");
const kind = computed(() => {
  const s = props.status;
  if (!props.on) return "off";
  if (s?.step) return "getting";
  if (s?.domain && s.valid_until && s.valid_until > now()) return "valid";
  if (s?.error) return "failed";
  return "waiting";
});

async function agreement() {
  if ("__TAURI_INTERNALS__" in window) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(AGREEMENT);
  } else {
    window.open(AGREEMENT, "_blank", "noopener");
  }
}
</script>

<template>
  <div class="space-y-1 text-sm" data-https>
    <p v-if="kind === 'valid'" class="flex items-start gap-1.5 text-ok">
      <Lock :size="15" class="mt-0.5 shrink-0" />{{ t("friends.httpsValid", { domain: status!.domain, until: date(status!.valid_until), renew: ago(status!.renew_at, locale) }) }}
    </p>
    <p v-else-if="kind === 'getting'" class="flex items-start gap-1.5 text-muted">
      <Spinner :size="14" class="spin mt-0.5 shrink-0" />{{ t("friends.httpsGetting", { step: t(`friends.httpsSteps.${status!.step}`) }) }}
    </p>
    <p v-else-if="kind === 'failed'" class="flex items-start gap-1.5 text-warn">
      <Warning :size="15" class="mt-0.5 shrink-0" />{{ t("friends.httpsFailed", { error: status!.error, next: ago(status!.next_try, locale) }) }}
    </p>
    <p v-else-if="kind === 'off'" class="flex items-start gap-1.5 text-warn"><Warning :size="15" class="mt-0.5 shrink-0" />{{ t("friends.httpsOff") }}</p>
    <p v-else class="text-muted">{{ t("friends.httpsWaiting") }}</p>
    <p v-if="status?.staging" class="text-xs text-warn">{{ t("friends.httpsStaging") }}</p>
    <p v-if="on" class="text-xs text-muted">
      <i18n-t keypath="friends.httpsAgreementText" scope="global">
        <template #link><a href="https://letsencrypt.org/repository/" class="underline" @click.prevent="agreement">{{ t("friends.httpsAgreement") }}</a></template>
      </i18n-t>
    </p>
  </div>
</template>
