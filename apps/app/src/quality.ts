// Readable texts for quality check issues.
import type { QualityIssue } from "./api";

type T = (key: string, params?: Record<string, unknown>) => string;

/** "Pitch high for the role (375 Hz)", "Sounds different in English (120 Hz there)" … */
export function issueText(t: T, i: QualityIssue): string {
  const v = i.value;
  const value = ["quiet", "loud", "new_level", "pause"].includes(i.code) ? v.toFixed(1) : ["slow", "fast", "misheard", "new_misheard"].includes(i.code) ? `${Math.round(v * 100)} %` : String(Math.round(v));
  const lang = i.extra && /^[a-z]{2}$/.test(i.extra) ? t(`lang.${i.extra}`) : (i.extra ?? "");
  const key = `quality.issue.${i.code}`;
  const text = t(key, { value, lang });
  return text === key ? i.code : text;
}

/** All issues of a voice in one line (tooltips). */
export function issuesLine(t: T, issues: QualityIssue[]): string {
  return issues.map((i) => issueText(t, i)).join(" · ");
}
