import { defineComponent, h, type PropType } from "vue";
import { nucleo } from "./nucleo";

// Inline Nucleo icons as tiny Vue components. Mono icons use currentColor;
// their second color (`.nc-c2`) follows the theme's cyan accent.
function icon(name: string) {
  return defineComponent({
    name: `Icon${name}`,
    props: { size: { type: [Number, String] as PropType<number | string>, default: 18 } },
    setup(props) {
      const [viewBox, body] = nucleo[name] ?? ["0 0 24 24", ""];
      return () =>
        h("svg", {
          xmlns: "http://www.w3.org/2000/svg",
          viewBox,
          width: props.size,
          height: props.size,
          class: "nc-icon",
          "aria-hidden": "true",
          innerHTML: body,
        });
    },
  });
}

export const ArrowRight = icon("ArrowRight");
export const Bolt = icon("Bolt");
export const Check = icon("Check");
export const ChevronDown = icon("ChevronDown");
export const ChevronRight = icon("ChevronRight");
export const CircleCheck = icon("CircleCheck");
export const CircleInfo = icon("CircleInfo");
export const CircleX = icon("CircleX");
export const Clock = icon("Clock");
export const Console = icon("Console");
export const Copy = icon("Copy");
export const Devices = icon("Devices");
export const Download = icon("Download");
export const ExternalLink = icon("ExternalLink");
export const Eye = icon("Eye");
export const EyeSlash = icon("EyeSlash");
export const FolderOpen = icon("FolderOpen");
export const Gauge = icon("Gauge");
export const Gear = icon("Gear");
export const Globe = icon("Globe");
export const Key = icon("Key");
export const Link = icon("Link");
export const Lock = icon("Lock");
export const Microchip = icon("Microchip");
export const Monitor = icon("Monitor");
export const Moon = icon("Moon");
export const Network = icon("Network");
export const Play = icon("Play");
export const Plus = icon("Plus");
export const Power = icon("Power");
export const Ram = icon("Ram");
export const Refresh = icon("Refresh");
export const Server = icon("Server");
export const ShieldCheck = icon("ShieldCheck");
export const Sparkle = icon("Sparkle");
export const Spinner = icon("Spinner");
export const Stop = icon("Stop");
export const Sun = icon("Sun");
export const Temperature = icon("Temperature");
export const Trash = icon("Trash");
export const Warning = icon("Warning");
export const Waveform = icon("Waveform");
export const Wifi = icon("Wifi");
export const X = icon("X");
