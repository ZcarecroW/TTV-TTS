/** 1536 → "1.5 KB", 8 589 934 592 → "8.0 GB" (binary units, like task managers). */
export function bytes(n: number | null | undefined, locale?: string): string {
  if (n == null || !Number.isFinite(n)) return "–";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString(locale, { maximumFractionDigits: v < 10 && i > 0 ? 1 : 0 })} ${units[i]}`;
}

/** 3725 → "1 h 2 min", 95 → "1 min 35 s". */
export function duration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h >= 48) return `${Math.floor(h / 24)} d ${h % 24} h`;
  if (h > 0) return `${h} h ${m} min`;
  if (m > 0) return `${m} min ${s % 60} s`;
  return `${s} s`;
}

/** Unix seconds → relative "2 min ago" in the given locale. */
export function ago(unixSecs: number | null | undefined, locale: string): string {
  if (!unixSecs) return "–";
  const diff = unixSecs - Date.now() / 1000;
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const abs = Math.abs(diff);
  if (abs < 60) return rtf.format(Math.round(diff), "second");
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  return rtf.format(Math.round(diff / 86400), "day");
}
