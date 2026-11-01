<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import UiField from "@ttv-tts/ui/components/UiField.vue";
import UiModal from "@ttv-tts/ui/components/UiModal.vue";
import UiSegmented from "@ttv-tts/ui/components/UiSegmented.vue";
import UiToggle from "@ttv-tts/ui/components/UiToggle.vue";
import { copyText } from "@ttv-tts/ui/clipboard";
import { CircleCheck, Copy, Plus, Spinner, Warning } from "@ttv-tts/ui/icons";
import { presets, type Parsed } from "../api";
import { loadPresets, run, store, toast } from "../store";

// Creates an own preset (id empty) or edits one. Two ways in: let any AI
// chat write the voices from a prompt built here, or type/paste the lines.
const props = defineProps<{ presetId?: string }>();
const emit = defineEmits<{ close: []; saved: [id: string] }>();
const { t } = useI18n();

const mode = ref<"ai" | "text">(props.presetId ? "text" : "ai");
const name = ref("");
const description = ref("");
const theme = ref("");
const total = ref(60);
const effects = ref(false);
const text = ref("");
const answer = ref("");
const parsed = ref<Parsed | null>(null);
const prompt = ref("");

onMounted(async () => {
  if (!props.presetId) return;
  const s = await run("export", () => presets.exportText(props.presetId!));
  if (s) {
    text.value = s;
    const p = await presets.parse(s);
    name.value = p.name;
    description.value = p.description;
    total.value = Math.max(p.defs.length, 1);
  }
});

// Check the text as it changes (cheap: parsing runs in Rust in < 1 ms).
let timer = 0;
watch(text, (s) => {
  clearTimeout(timer);
  timer = window.setTimeout(async () => {
    parsed.value = s.trim() ? await presets.parse(s) : null;
    if (parsed.value?.name && !name.value) name.value = parsed.value.name;
    if (parsed.value?.description && !description.value) description.value = parsed.value.description;
  }, 250);
});

const have = computed(() => parsed.value?.defs ?? []);
const errors = computed(() => parsed.value?.issues.filter((i) => i.level === "error") ?? []);
const warnings = computed(() => parsed.value?.issues.filter((i) => i.level === "warning") ?? []);
const remaining = computed(() => Math.max(0, total.value - have.value.length));
const part = computed(() => Math.floor(have.value.length / 60) + 1);
const parts = computed(() => Math.max(1, Math.ceil(total.value / 60)));

// The prompt follows what is already there (next part, names to avoid).
let promptSeq = 0;
watch(
  [theme, name, total, effects, have],
  async () => {
    const seq = ++promptSeq;
    if (!theme.value.trim()) return (prompt.value = "");
    const p = await presets.prompt({ theme: theme.value, name: name.value, total: total.value, have: have.value, effects: effects.value });
    if (seq === promptSeq) prompt.value = p;
  },
  { deep: false },
);

async function copyPrompt() {
  if (await copyText(prompt.value)) toast("ok", t("editor.promptCopied"));
}

/** Adds an AI answer to what is there. */
function addAnswer() {
  const a = answer.value.trim();
  if (!a) return;
  text.value = text.value.trim() ? `${text.value.trim()}\n${a}\n` : `${a}\n`;
  answer.value = "";
}

const canSave = computed(() => have.value.length > 0 && !!name.value.trim());

async function save() {
  const view = await run("savePreset", () =>
    presets.save({ id: props.presetId ?? "", name: name.value.trim(), description: description.value.trim(), icon: "", builtin: false, target: total.value, defs: have.value }),
  );
  if (view) {
    await loadPresets();
    toast("ok", t("editor.saved", { name: view.name, n: view.count }));
    emit("saved", view.id);
  }
}
</script>

<template>
  <UiModal :title="presetId ? t('editor.editTitle') : t('editor.newTitle')" width="max-w-3xl" @close="emit('close')">
    <div class="space-y-5">
      <div class="grid gap-3 sm:grid-cols-[1fr_2fr]">
        <UiField :label="t('editor.name')"><input v-model="name" class="input" maxlength="60" :placeholder="t('editor.namePlaceholder')" /></UiField>
        <UiField :label="t('editor.description')"><input v-model="description" class="input" maxlength="200" :placeholder="t('editor.descriptionPlaceholder')" /></UiField>
      </div>

      <UiSegmented
        v-model="mode"
        :options="[
          { value: 'ai', label: t('editor.withAi') },
          { value: 'text', label: t('editor.byHand') },
        ]"
      />

      <template v-if="mode === 'ai'">
        <ol class="ml-4 list-decimal space-y-1 text-sm text-muted">
          <li>{{ t("editor.step1") }}</li>
          <li>{{ t("editor.step2") }}</li>
          <li>{{ t("editor.step3") }}</li>
        </ol>
        <div class="grid gap-3 sm:grid-cols-[2fr_1fr]">
          <UiField :label="t('editor.theme')" :tip="t('tips.theme')"><input v-model="theme" class="input" :placeholder="t('editor.themePlaceholder')" /></UiField>
          <UiField :label="t('editor.total')" :tip="t('tips.total')"><input v-model.number="total" type="number" min="1" max="1000" class="input" /></UiField>
        </div>
        <UiToggle v-model="effects" :label="t('editor.effects')" :tip="t('tips.effects')" />
        <div v-if="prompt">
          <div class="mb-1.5 flex items-center gap-2">
            <span class="label mb-0">{{ remaining ? t("editor.promptFor", { part, parts }) : t("editor.promptDone") }}</span>
            <button class="btn btn-sm ml-auto" :disabled="!remaining" @click="copyPrompt"><Copy :size="14" />{{ t("editor.copyPrompt") }}</button>
          </div>
          <textarea :value="prompt" readonly rows="6" class="input code h-auto py-2 text-xs" />
        </div>
        <UiField :label="t('editor.answer')">
          <textarea v-model="answer" rows="5" class="input code h-auto py-2 text-xs" :placeholder="t('editor.answerPlaceholder')" />
        </UiField>
        <button class="btn" :disabled="!answer.trim()" @click="addAnswer"><Plus :size="15" />{{ t("editor.addAnswer") }}</button>
      </template>

      <UiField v-else :label="t('editor.lines')" :tip="t('tips.lineFormat')">
        <textarea v-model="text" rows="14" class="input code h-auto py-2 text-xs" spellcheck="false" :placeholder="t('editor.linesPlaceholder')" />
      </UiField>

      <div v-if="parsed" class="rounded-xl border border-line bg-panel-2/40 p-3.5 text-sm">
        <div class="flex flex-wrap items-center gap-2">
          <CircleCheck :size="16" class="text-ok" />
          <span class="font-medium">{{ t("editor.voices", { n: have.length, total }) }}</span>
          <span v-if="errors.length" class="badge badge-err">{{ t("editor.errors", errors.length) }}</span>
          <span v-if="warnings.length" class="badge badge-warn">{{ t("editor.warnings", warnings.length) }}</span>
          <button v-if="mode === 'ai'" class="btn btn-ghost btn-sm ml-auto" @click="mode = 'text'">{{ t("editor.showLines") }}</button>
        </div>
        <ul v-if="parsed.issues.length" class="mt-2 max-h-40 space-y-1 overflow-y-auto text-xs">
          <li v-for="(i, k) in parsed.issues.slice(0, 50)" :key="k" class="flex gap-2" :class="i.level === 'error' ? 'text-err' : 'text-warn'">
            <Warning :size="13" class="mt-px" /><span>{{ i.line ? t("editor.line", { n: i.line }) : "" }} {{ i.message }}</span>
          </li>
        </ul>
        <p v-if="errors.length" class="hint mt-2">{{ t("editor.errorsSkipped") }}</p>
      </div>
    </div>

    <template #footer>
      <button class="btn" @click="emit('close')">{{ t("common.cancel") }}</button>
      <button class="btn btn-primary" :disabled="!canSave || store.busy === 'savePreset'" @click="save">
        <Spinner v-if="store.busy === 'savePreset'" :size="15" class="spin" />{{ t("editor.save", { n: have.length }) }}
      </button>
    </template>
  </UiModal>
</template>
