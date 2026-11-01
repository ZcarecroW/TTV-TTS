<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import { CircleCheck, Lock, Monitor, Network, Refresh, Server, Spinner } from "@ttv-tts/ui/icons";
import { api, type Found, type Probe } from "../api";
import { apply, run, store, toast } from "../store";

const emit = defineEmits<{ done: [] }>();
const { t } = useI18n();

const mode = ref<"proxy" | "direct">("proxy");
const found = ref<Found[] | null>(null);
const searching = ref(false);
const manual = ref("");
const probe = ref<Probe | null>(null);
const code = ref("");
const directUrl = ref("http://127.0.0.1:8100");
const directKey = ref("");

async function search() {
  searching.value = true;
  try {
    found.value = await api.discover();
  } finally {
    searching.value = false;
  }
}
onMounted(search);

async function choose(address: string) {
  const p = await run("probe", () => api.probe(address));
  if (p) {
    code.value = "";
    probe.value = p;
  }
}

async function pair() {
  const p = probe.value;
  if (!p) return;
  const v = await run("pair", () => api.pair(p.address, p.fingerprint, code.value));
  if (v) {
    await apply(v);
    toast("ok", t("host.paired", { name: p.info.name }));
    probe.value = null;
    emit("done");
  }
}

async function useDirect() {
  const v = await run("direct", () => api.useDirect(directUrl.value, directKey.value || null));
  if (v) {
    await apply(v);
    emit("done");
  }
}
</script>

<template>
  <div class="space-y-5">
    <div class="grid gap-3 md:grid-cols-2">
      <button
        v-for="m in (['proxy', 'direct'] as const)"
        :key="m"
        type="button"
        class="card flex items-start gap-3 p-4 text-left transition-colors"
        :class="mode === m ? 'border-accent ring-2 ring-accent-soft' : 'hover:bg-panel-2'"
        @click="mode = m"
      >
        <div class="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent-text">
          <Server v-if="m === 'proxy'" :size="20" /><Monitor v-else :size="20" />
        </div>
        <div>
          <div class="font-semibold">{{ m === "proxy" ? t("host.proxyCard") : t("host.directCard") }}</div>
          <div class="mt-0.5 text-sm text-muted">{{ m === "proxy" ? t("host.proxyCardText") : t("host.directCardText") }}</div>
        </div>
      </button>
    </div>

    <section v-if="mode === 'proxy'" class="card p-5">
      <div class="card-head">
        <Network />{{ t("host.found") }}
        <button class="btn btn-ghost btn-sm ml-auto" :disabled="searching" @click="search">
          <Refresh :size="15" :class="searching && 'spin'" />{{ searching ? t("common.searching") : t("common.search") }}
        </button>
      </div>
      <p v-if="searching && !found" class="flex items-center gap-2 text-sm text-muted"><Spinner :size="15" class="spin" />{{ t("common.searching") }}</p>
      <p v-else-if="found && !found.length" class="text-sm text-muted">{{ t("host.foundNone") }}</p>
      <ul v-else class="space-y-2">
        <li v-for="f in found" :key="f.info.id">
          <button type="button" class="flex w-full items-center gap-3 rounded-xl border border-line bg-panel-2 p-3 text-left hover:border-accent" @click="choose(f.address)">
            <Server :size="20" class="text-accent-text" />
            <div class="min-w-0 flex-1">
              <div class="font-medium">{{ f.info.name }}</div>
              <div class="text-xs text-muted">{{ f.address }} · TTV-TTS Proxy {{ f.info.version }}</div>
            </div>
            <span class="badge" :class="f.info.vcp_reachable ? 'badge-ok' : 'badge-warn'">VCP {{ f.info.vcp_version ?? "–" }}</span>
          </button>
        </li>
      </ul>
      <div class="mt-4">
        <label class="label" for="manual">{{ t("host.manual") }}</label>
        <div class="flex max-w-md gap-2">
          <input id="manual" v-model="manual" class="input mono" :placeholder="t('host.manualPlaceholder')" @keydown.enter="manual && choose(manual)" />
          <button class="btn" :disabled="!manual.trim() || store.busy === 'probe'" @click="choose(manual)">{{ t("common.connect") }}</button>
        </div>
      </div>
    </section>

    <section v-else class="card p-5">
      <div class="grid max-w-xl gap-4">
        <div>
          <label class="label" for="durl">{{ t("host.directUrl") }}</label>
          <input id="durl" v-model="directUrl" class="input mono" />
        </div>
        <div>
          <label class="label" for="dkey">{{ t("host.directKey") }}</label>
          <input id="dkey" v-model="directKey" type="password" autocomplete="off" class="input mono" :placeholder="t('host.directKeyPlaceholder')" />
        </div>
        <div><button class="btn btn-primary" :disabled="store.busy === 'direct'" @click="useDirect">{{ t("common.connect") }}</button></div>
      </div>
    </section>

    <UiModal v-if="probe" :title="t('host.pairTitle', { name: probe.info.name })" width="max-w-md" @close="probe = null">
      <p class="text-sm">{{ t("host.pairCompare") }}</p>
      <div class="my-3 flex items-center justify-center gap-2 rounded-xl bg-panel-2 p-3">
        <Lock :size="18" class="text-accent-text" /><code class="font-mono text-lg font-semibold">{{ probe.fingerprint_short }}</code>
      </div>
      <label class="label" for="code">{{ t("host.pairCode") }}</label>
      <input
        id="code"
        v-model="code"
        inputmode="numeric"
        maxlength="6"
        autocomplete="one-time-code"
        class="input mono text-center text-2xl tracking-[0.4em]"
        placeholder="000000"
        @keydown.enter="code.length === 6 && pair()"
      />
      <p class="hint">{{ t("host.pairHint") }}</p>
      <template #footer>
        <button class="btn" @click="probe = null">{{ t("common.cancel") }}</button>
        <button class="btn btn-primary" :disabled="code.trim().length !== 6 || store.busy === 'pair'" @click="pair"><CircleCheck :size="16" />{{ t("host.pairButton") }}</button>
      </template>
    </UiModal>
  </div>
</template>
