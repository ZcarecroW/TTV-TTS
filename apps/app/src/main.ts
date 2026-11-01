import { createApp } from "vue";
import "@ttv-tts/ui/style.css";
import App from "./App.vue";
import { i18n } from "./i18n";
import { init } from "./store";

// Load state and language first, then show the UI in one go (no flicker).
init()
  .catch((e) => console.error("init failed", e))
  .finally(() => createApp(App).use(i18n).mount("#app"));
