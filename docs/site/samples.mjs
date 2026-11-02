// Records the voice samples on the home page: for every built-in preset one
// line in English and one in German, spoken by two of its computed voices
// through Voice Creator Pro (gain and the voice's effect as in the app),
// saved as MP3 in docs/samples with docs/samples/samples.json.
//   node docs/site/samples.mjs [--force] [preset names …]
// Needs the app running as a development build with remote debugging
// (npx tauri dev --config src-tauri/tauri.e2e.conf.json in apps/app) and the
// MP3 encoder (npm install wasm-media-encoders in TTV_ENCODER_DIR, default
// .work/enc). Presets whose voices aren't computed yet are left out; run it
// again later. Existing samples are kept unless --force.
import { chromium } from "playwright-core";
import { createRequire } from "node:module";
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
const out = join(root, "docs/samples");
const encDir = process.env.TTV_ENCODER_DIR ?? join(root, ".work/enc");
const { createMp3Encoder } = createRequire(join(encDir, "package.json"))("wasm-media-encoders");
const args = process.argv.slice(2);
const force = args.includes("--force");
const only = args.filter((a) => !a.startsWith("--"));

// preset name -> [German description, [voice index, English line], [voice index, German line]]
const LINES = {
  Cinematic: ["Trailer-Sprecher, Erzähler, Helden und Schurken der großen Leinwand", [0, "In a world where chat never sleeps, one streamer dared to press go live."], [1, "In einer Welt, in der der Chat niemals schläft, wagte es ein Streamer, live zu gehen."]],
  "Everyday Neighbors": ["Nachbarn, Kollegen, Verwandte – ganz normale Leute von nebenan", [0, "Morning! Just popped in to say the stream looks great today. Carry on!"], [1, "Hallo zusammen! Ich wollte nur kurz sagen: Der Stream sieht heute richtig gut aus."]],
  "Fantasy Tavern": ["Wirte, Barden, Abenteurer, Zauberer und schräge Stammgäste am Kamin", [0, "Welcome, traveller! Pull up a stool. The ale is fresh and the stories are long."], [1, "Willkommen, Reisende! Setzt euch zu uns, das Bier ist frisch und die Geschichten sind lang."]],
  "Starship Crew": ["Captains, Ingenieure, Bordcomputer und Außerirdische an Bord", [0, "Captain, sensors show three new followers off the starboard bow."], [1, "Captain, die Sensoren melden drei neue Follower an Steuerbord."]],
  "Noir City": ["Detektive, Gangster, Sängerinnen und Informanten im Regen der Großstadt", [0, "The rain hadn't stopped in three days. Neither had the chat."], [1, "Seit drei Tagen hatte es nicht aufgehört zu regnen. Der Chat auch nicht."]],
  "Saturday Cartoons": ["Quirlige Zeichentrickfiguren, Bösewichte und Sidekicks vom Samstagmorgen", [0, "Ooh, ooh! Did you see that jump? Do it again, do it again!"], [1, "Oh, hast du den Sprung gesehen? Nochmal, nochmal!"]],
  "High Seas": ["Kapitäne, Piraten, Matrosen und Hafenwirte auf hoher See", [0, "Arr, hoist the colours, lads! The streamer has found the treasure at last!"], [1, "Ahoi! Hisst die Flagge, Leute, der Streamer hat endlich den Schatz gefunden!"]],
  "Royal Court": ["Könige, Königinnen, Ritter, Hofnarren und Intriganten bei Hofe", [0, "By royal decree, this stream shall continue until the boss is defeated."], [1, "Auf königlichen Erlass soll dieser Stream weitergehen, bis der Boss besiegt ist."]],
  "Midnight Horror": ["Unheimliche Erzähler, Geister, verrückte Wissenschaftler und Überlebende", [0, "Did you hear that? Something is moving behind the door. Don't open it."], [1, "Hast du das gehört? Hinter der Tür bewegt sich etwas. Mach sie nicht auf."]],
  "Arena Casters": ["E-Sport-Caster, Sportkommentatoren, Trainer, Profis und Superfans", [0, "And he goes for it! What a play! The crowd is on its feet!"], [1, "Und er wagt es! Was für ein Spielzug! Das Publikum ist völlig aus dem Häuschen!"]],
  "On Air": ["Radiomoderatoren, Nachrichtensprecher, Werbestimmen und Anrufer", [0, "You're listening to the late show, and our next caller has a question about that last boss."], [1, "Ihr hört die Spätsendung, und unser nächster Anrufer hat eine Frage zum letzten Boss."]],
  "Wild Planet": ["Naturfilm-Erzähler, Ranger, Forscher und Abenteurer in der Wildnis", [0, "Here, in the quiet of the forest, the streamer prepares for the final fight."], [1, "Hier, in der Stille des Waldes, bereitet sich der Streamer auf den letzten Kampf vor."]],
  Storytime: ["Märchenerzähler, Großeltern, Fabelwesen und Figuren aus Kinderbüchern", [0, "Once upon a time, in a chat not so far away, everyone said hello at once."], [1, "Es war einmal ein Chat, nicht weit von hier, in dem alle gleichzeitig Hallo sagten."]],
  "Frontier West": ["Sheriffs, Revolverhelden, Saloon-Wirte und Goldsucher im Wilden Westen", [0, "Well howdy, partner. Reckon this here stream's the finest in the whole territory."], [1, "Na, Partner? Ich schätze, das ist der beste Stream im ganzen Territorium."]],
  "Neon Streets": ["Hacker, Söldner, Konzernleute und Straßenkids einer Neon-Zukunft", [0, "Jacked in and ready. Ping me when the next run starts."], [1, "Eingeloggt und bereit. Sag Bescheid, wenn der nächste Run startet."]],
  Olympus: ["Götter, Helden, Orakel und Ungeheuer der alten Sagen", [0, "Hear me, mortals! The gods themselves are watching this stream tonight."], [1, "Hört mich an, Sterbliche! Heute Abend schauen sogar die Götter diesem Stream zu."]],
  "Capes & Masks": ["Eigene Superhelden, Schurken, Sidekicks, Reporter und Bürger einer Comic-Stadt", [0, "Fear not, citizens! No lag can stop this hero!"], [1, "Keine Angst, Bürger! Kein Lag kann diesen Helden aufhalten!"]],
  "Grand Stage": ["Bühnenstars, Opernsänger, Zirkusdirektoren und Theaterleute", [0, "Ladies and gentlemen, the curtain rises on tonight's grand performance!"], [1, "Meine Damen und Herren, der Vorhang hebt sich für die große Vorstellung des Abends!"]],
  "Quest Givers": ["Händler, Dorfälteste, Wachen und Auftraggeber aus Rollenspielen", [0, "Ah, adventurer! Bring me ten wolf pelts and I shall reward you handsomely."], [1, "Ah, Abenteurer! Bring mir zehn Wolfsfelle, und ich werde dich reich belohnen."]],
  "Calm & Cozy": ["Sanfte, beruhigende Stimmen für entspannte Streams, Regenabende und späte Chats", [0, "Take a breath, grab some tea, and enjoy the rain with us tonight."], [1, "Atme tief durch, hol dir einen Tee und genieß heute Abend mit uns den Regen."]],
};

async function mp3(pcm16b64, rate) {
  const b = Buffer.from(pcm16b64, "base64");
  const f = new Float32Array(b.length / 2);
  for (let i = 0; i < f.length; i++) f[i] = b.readInt16LE(i * 2) / 32768;
  const enc = await createMp3Encoder();
  enc.configure({ sampleRate: rate, channels: 1, vbrQuality: 5 });
  const parts = [];
  for (let i = 0; i < f.length; i += 1152 * 32) parts.push(Buffer.from(enc.encode([f.subarray(i, i + 1152 * 32)])));
  parts.push(Buffer.from(enc.finalize()));
  return Buffer.concat(parts);
}

const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const page = browser.contexts().flatMap((c) => c.pages()).find((p) => /localhost:1440|tauri\.localhost/.test(p.url()));
const call = (cmd, a = {}) => page.evaluate(([c, x]) => window.__TAURI_INTERNALS__.invoke(c, x), [cmd, a]);
mkdirSync(out, { recursive: true });
const manifestPath = join(out, "samples.json");
const old = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, "utf8")) : [];
const result = [];
try {
  const presets = await call("list_presets");
  for (const [name, [descDe, enLine, deLine]] of Object.entries(LINES)) {
    const p = presets.find((x) => x.name === name && x.builtin !== false);
    if (!p) {
      console.log(`${name}: no such preset`);
      continue;
    }
    const keep = old.find((o) => o.name.en === name);
    if ((only.length && !only.includes(name)) || (!force && keep?.clips.length === 2 && keep.clips.every((c) => existsSync(join(out, c.file))))) {
      if (keep) result.push(keep);
      continue;
    }
    const clips = [];
    for (const [lang, [index, text]] of [["en", enLine], ["de", deLine]]) {
      const file = `${p.id}-${lang}.mp3`;
      try {
        const t0 = Date.now();
        const s = await call("debug_sample", { presetId: p.id, index, lang, text });
        const data = await mp3(s.pcm16, s.rate);
        writeFileSync(join(out, file), data);
        clips.push({ lang, voice: s.voice, text, file });
        console.log(`${file}: ${s.voice}, ${(Buffer.from(s.pcm16, "base64").length / 2 / s.rate).toFixed(1)} s, ${(data.length / 1024).toFixed(0)} KB, ${Date.now() - t0} ms`);
      } catch (e) {
        console.log(`${name} (${lang}): ${String(e).split("\n")[0]}`);
      }
    }
    if (clips.length) result.push({ preset: p.id, name: { en: name, de: name }, desc: { en: p.desc ?? p.description ?? "", de: descDe }, icon: p.icon, clips });
  }
} finally {
  await browser.close();
}
// in the order of LINES
const order = Object.keys(LINES);
result.sort((a, b) => order.indexOf(a.name.en) - order.indexOf(b.name.en));
writeFileSync(manifestPath, JSON.stringify(result, null, 1) + "\n");
const total = result.flatMap((p) => p.clips).reduce((n, c) => n + statSync(join(out, c.file)).size, 0);
console.log(`${result.length} presets, ${result.flatMap((p) => p.clips).length} samples, ${(total / 1024).toFixed(0)} KB`);
