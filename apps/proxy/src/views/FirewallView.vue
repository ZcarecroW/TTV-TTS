<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import UiCopy from "@ttv-tts/ui/components/UiCopy.vue";
import { ChevronRight, CircleCheck, CircleInfo, CircleX, Refresh, ShieldCheck, Warning, Wifi } from "@ttv-tts/ui/icons";
import { api, errText } from "../api";
import { checkFirewall, config, status, store, toast } from "../store";
import UiDocs from "@ttv-tts/ui/components/UiDocs.vue";

const { t } = useI18n();
const fw = computed(() => store.firewall);
const port = computed(() => config.value!.port);
const ip = computed(() => status.value!.addresses[0]?.ip ?? "192.168.x.x");

type Row = { label: string; value: string; ok: boolean | null; note?: string };
const rows = computed<Row[]>(() => {
  const f = fw.value;
  if (!f || !f.managed) return [];
  const out: Row[] = [];
  const r = f.rule;
  out.push({
    label: t("firewall.rule"),
    value: !r ? t("firewall.ruleMissing") : !r.enabled ? t("firewall.ruleOff") : f.rule_ok ? t("firewall.ruleOk", { profile: r.profile }) : t("firewall.ruleOther"),
    // Windows' own popup may have allowed it too: no rule of ours is fine then.
    ok: f.rule_ok ? true : f.allows > 0 ? null : false,
  });
  out.push({ label: t("firewall.blocks"), value: f.blocks ? t("firewall.blocksFound", f.blocks) : t("firewall.blocksNone"), ok: f.blocks === 0 });
  for (const p of f.profiles) {
    const blocked = p.category === "Public" && f.public_network_blocked;
    out.push({
      label: t("firewall.network"),
      value: blocked ? t("firewall.networkPublic", { alias: p.alias }) : t("firewall.networkOk", { alias: p.alias, category: p.category }),
      ok: !blocked,
    });
  }
  if (f.wfc) out.push({ label: t("firewall.wfc"), value: t("firewall.wfcText"), ok: null });
  out.push({ label: t("firewall.thirdParty"), value: f.third_party.length ? f.third_party.join(", ") : t("firewall.thirdPartyNone"), ok: f.third_party.length ? null : true });
  return out;
});

async function fix(all: boolean) {
  try {
    store.firewall = await api.firewallFix(all);
    toast("ok", t("firewall.allowed"));
  } catch (e) {
    const msg = errText(e);
    toast(msg === "cancelled" ? "info" : "err", msg === "cancelled" ? t("firewall.cancelled") : msg);
  }
}

const guides = ["windows", "wfc", "suites", "macos", "linux", "router"] as const;
const psTest = computed(() => `Test-NetConnection ${ip.value} -Port ${port.value}`);
const infoUrl = computed(() => `https://${ip.value}:${port.value}/ttv/v1/info`);
</script>

<template>
  <div class="space-y-5">
    <section class="card p-5">
      <div class="card-head">
        <ShieldCheck />{{ t("firewall.statusTitle") }}
        <button class="btn btn-ghost btn-sm ml-auto" :disabled="store.firewallChecking" @click="checkFirewall()">
          <Refresh :size="15" :class="store.firewallChecking && 'spin'" />{{ store.firewallChecking ? t("common.checking") : t("common.check") }}
        </button>
      </div>
      <p class="muted mb-3 text-sm">{{ t("firewall.statusText", { port }) }}</p>
      <p v-if="fw && !fw.managed" class="notice notice-info"><CircleInfo :size="18" /><span>{{ t("firewall.notManaged") }}</span></p>
      <p v-else-if="fw?.error" class="notice notice-err"><CircleX :size="18" /><span>{{ fw.error }}</span></p>
      <ul v-else-if="rows.length" class="divide-y divide-line rounded-xl border border-line">
        <li v-for="(r, i) in rows" :key="i" class="flex items-start gap-3 px-3.5 py-2.5 text-sm">
          <CircleCheck v-if="r.ok === true" class="mt-0.5 text-ok" />
          <CircleX v-else-if="r.ok === false" class="mt-0.5 text-err" />
          <Warning v-else class="mt-0.5 text-warn" />
          <span class="w-56 shrink-0 font-medium">{{ r.label }}</span>
          <span class="muted">{{ r.value }}</span>
        </li>
      </ul>
      <p v-else class="muted text-sm">{{ t("common.checking") }}</p>
      <div v-if="fw?.managed" class="mt-4 flex flex-wrap gap-2">
        <button class="btn btn-primary" @click="fix(false)"><ShieldCheck :size="16" />{{ t("firewall.allowPrivate") }}</button>
        <button class="btn" :data-tip="t('tips.allowAll')" @click="fix(true)">{{ t("firewall.allowAll") }}</button>
      </div>
      <div v-if="fw?.program" class="mt-4 text-xs">
        <span class="muted">{{ t("firewall.program") }}:</span>
        <code class="ml-1 break-all font-mono">{{ fw.program }}</code>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><CircleInfo />{{ t("firewall.guidesTitle") }}<UiDocs class="ml-auto" page="firewall" /></div>
      <div class="divide-y divide-line rounded-xl border border-line">
        <details v-for="g in guides" :key="g" class="group">
          <summary class="flex cursor-pointer list-none items-center gap-2 px-3.5 py-2.5 text-sm font-medium hover:bg-panel-2">
            <ChevronRight :size="16" class="text-muted transition-transform group-open:rotate-90" />
            {{ t(`firewall.guides.${g}.title`) }}
          </summary>
          <p class="whitespace-pre-line px-3.5 pb-3 pl-10 text-sm leading-relaxed">{{ t(`firewall.guides.${g}.body`, { port }) }}</p>
        </details>
      </div>
    </section>

    <section class="card p-5">
      <div class="card-head"><Wifi />{{ t("firewall.testTitle") }}</div>
      <p class="text-sm">{{ t("firewall.testText", { name: status!.info.name }) }}</p>
      <div class="mt-2 flex items-center gap-1 rounded-lg bg-panel-2 py-0.5 pl-2.5 pr-0.5">
        <code class="flex-1 truncate font-mono text-sm">{{ psTest }}</code><UiCopy :value="psTest" />
      </div>
      <p class="mt-3 text-sm">{{ t("firewall.testBrowser") }}</p>
      <div class="mt-2 flex items-center gap-1 rounded-lg bg-panel-2 py-0.5 pl-2.5 pr-0.5">
        <code class="flex-1 truncate font-mono text-sm">{{ infoUrl }}</code><UiCopy :value="infoUrl" />
      </div>
    </section>
  </div>
</template>
