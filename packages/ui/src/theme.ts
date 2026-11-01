// Light/dark mode: "system" follows the OS and switches live with it.
const media = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
let current = "system";

function apply() {
  const dark = current === "dark" || (current === "system" && !!media?.matches);
  document.documentElement.classList.toggle("dark", dark);
}

media?.addEventListener("change", () => current === "system" && apply());

export function setTheme(setting: string) {
  current = setting === "dark" || setting === "light" ? setting : "system";
  apply();
}
