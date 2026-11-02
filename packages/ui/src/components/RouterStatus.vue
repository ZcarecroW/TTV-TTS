<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { CircleCheck, Network, Spinner, Warning } from "../icons";
import type { UpnpStatus } from "../friends";

// The ports opened in the router by UPnP, in one line.
const props = defineProps<{ status: UpnpStatus | null | undefined; on: boolean }>();
const { t } = useI18n();
const kind = computed(() => {
  const s = props.status;
  if (!props.on) return "off";
  if (s?.no_public_ip) return "nopublic";
  if (s?.error) return "failed";
  if (s?.mapped.length) return "open";
  if (!s?.wanted.length) return "idle";
  return "asking";
});
</script>

<template>
  <div class="text-sm" data-upnp>
    <p v-if="kind === 'open'" class="flex items-start gap-1.5 text-ok"><CircleCheck :size="15" class="mt-0.5 shrink-0" />{{ t("friends.upnpOpen", { ports: status!.mapped.join(", "), router: status!.router ?? "?" }) }}</p>
    <p v-else-if="kind === 'nopublic'" class="flex items-start gap-1.5 text-warn"><Warning :size="15" class="mt-0.5 shrink-0" />{{ t("friends.upnpNoPublic", { ip: status!.external_ip ?? "?" }) }}</p>
    <p v-else-if="kind === 'failed'" class="flex items-start gap-1.5 text-warn"><Warning :size="15" class="mt-0.5 shrink-0" />{{ status!.error }}</p>
    <p v-else-if="kind === 'off'" class="flex items-start gap-1.5 text-muted"><Network :size="15" class="mt-0.5 shrink-0" />{{ t("friends.upnpOff") }}</p>
    <p v-else-if="kind === 'asking'" class="flex items-start gap-1.5 text-muted"><Spinner :size="14" class="spin mt-0.5 shrink-0" />{{ t("friends.upnpAsking") }}</p>
    <p v-else class="text-muted">{{ t("friends.upnpIdle") }}</p>
  </div>
</template>
