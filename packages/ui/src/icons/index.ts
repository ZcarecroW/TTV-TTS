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

/** An icon picked by name at runtime (e.g. a preset's icon); unknown names get Waveform. */
const named = new Map<string, ReturnType<typeof icon>>();
export function iconByName(name: string) {
  const key = name in nucleo ? name : "Waveform";
  let c = named.get(key);
  if (!c) named.set(key, (c = icon(key)));
  return c;
}

export const Anchor = icon("Anchor");
export const ArrowRight = icon("ArrowRight");
export const Atlas = icon("Atlas");
export const BeerMug = icon("BeerMug");
export const Bolt = icon("Bolt");
export const BookOpen = icon("BookOpen");
export const Cactus = icon("Cactus");
export const Check = icon("Check");
export const ChevronDown = icon("ChevronDown");
export const ChevronRight = icon("ChevronRight");
export const CircleCheck = icon("CircleCheck");
export const CircleInfo = icon("CircleInfo");
export const CircleX = icon("CircleX");
export const Clapperboard = icon("Clapperboard");
export const Clock = icon("Clock");
export const ClockRotate = icon("ClockRotate");
export const Console = icon("Console");
export const Copy = icon("Copy");
export const Crown = icon("Crown");
export const Devices = icon("Devices");
export const Download = icon("Download");
export const ExternalLink = icon("ExternalLink");
export const Eye = icon("Eye");
export const EyeSlash = icon("EyeSlash");
export const FolderOpen = icon("FolderOpen");
export const Gauge = icon("Gauge");
export const Gear = icon("Gear");
export const Ghost = icon("Ghost");
export const Globe = icon("Globe");
export const House = icon("House");
export const Key = icon("Key");
export const Keyboard = icon("Keyboard");
export const Layers = icon("Layers");
export const Link = icon("Link");
export const Lock = icon("Lock");
export const Mask = icon("Mask");
export const Microchip = icon("Microchip");
export const Microphone = icon("Microphone");
export const Monitor = icon("Monitor");
export const Moon = icon("Moon");
export const Mug = icon("Mug");
export const Network = icon("Network");
export const Pause = icon("Pause");
export const Paw = icon("Paw");
export const Pen = icon("Pen");
export const Play = icon("Play");
export const Plus = icon("Plus");
export const Power = icon("Power");
export const Ram = icon("Ram");
export const Refresh = icon("Refresh");
export const Robot = icon("Robot");
export const Rocket = icon("Rocket");
export const Server = icon("Server");
export const ShieldCheck = icon("ShieldCheck");
export const Sparkle = icon("Sparkle");
export const Spinner = icon("Spinner");
export const Stop = icon("Stop");
export const Sun = icon("Sun");
export const Temperature = icon("Temperature");
export const Temple = icon("Temple");
export const TheatreMasks = icon("TheatreMasks");
export const TopHat = icon("TopHat");
export const Trash = icon("Trash");
export const Trophy = icon("Trophy");
export const Tv = icon("Tv");
export const Upload = icon("Upload");
export const Users = icon("Users");
export const Warning = icon("Warning");
export const Waveform = icon("Waveform");
export const Wifi = icon("Wifi");
export const X = icon("X");
