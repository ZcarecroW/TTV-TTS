// Why a message wasn't read (or was read differently): the backend sends a
// short English reason; this turns the known ones into the interface
// language. Anything else (an error from Voice Creator Pro) stays as it is.
import { i18n } from "./i18n";

const KEYS: Record<string, string> = {
  "not the chosen channel-points reward": "reward",
  "ignored user (bot)": "ignoredUser",
  muted: "muted",
  "broadcaster ignored": "broadcaster",
  "no allowed role": "role",
  "command / prefix": "prefix",
  cooldown: "cooldown",
  "contains a link": "link",
  "emotes only": "emotesOnly",
  "nothing to read (blocked word or only symbols)": "nothing",
  "too many waiting for approval": "approvalFull",
  "queue full": "queueFull",
  "not approved": "notApproved",
  "not approved in time": "approvalTimeout",
  "message deleted": "deleted",
  "timeout/ban": "timeout",
  "no voice host": "noHost",
  "no voice found - set a default voice in VCP or create voices": "noVoice",
  "system voice": "systemVoice",
  "remote control": "remote",
};

export function reasonText(reason: string): string {
  const t = i18n.global.t as (key: string, params?: Record<string, unknown>) => string;
  const known = (r: string) => (KEYS[r] ? t(`reasons.${KEYS[r]}`) : r);
  let m = reason.match(/^queue cleared \((.*)\)$/);
  if (m) return t("reasons.cleared", { by: known(m[1]!) });
  m = reason.match(/^(.*) – read by the system voice$/);
  if (m) return t("reasons.fallback", { why: known(m[1]!) });
  m = reason.match(/^not translated: (.*)$/);
  if (m) return t("reasons.notTranslated", { error: m[1]! });
  return known(reason);
}
