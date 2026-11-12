/** Dates and counts in the product voice: digits, short, sentence case. */

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function startOfDay(d: Date) {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** Compact relative date for list rows: "Now", "5m", "2h", "Yesterday", "Mon", "12 Oct", "12 Oct 2024". */
export function relativeDate(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  const t = date.getTime();
  if (Number.isNaN(t)) return "";
  const diff = now.getTime() - t;
  if (diff < MINUTE) return "Now";
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)}m`;
  const days = Math.round((startOfDay(now) - startOfDay(date)) / DAY);
  if (days === 0) return `${Math.floor(diff / HOUR)}h`;
  if (days === 1) return "Yesterday";
  if (days < 7) return date.toLocaleDateString("en-GB", { weekday: "short" });
  if (date.getFullYear() === now.getFullYear()) {
    return date.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
  }
  return date.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

/** "Today, 10:00", "Yesterday, 16:30", "Mon 3 Nov, 09:15". */
export function dayAndTime(iso: string, now: Date = new Date()): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const time = date.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  const days = Math.round((startOfDay(now) - startOfDay(date)) / DAY);
  if (days === 0) return `Today, ${time}`;
  if (days === 1) return `Yesterday, ${time}`;
  const sameYear = date.getFullYear() === now.getFullYear();
  const day = date.toLocaleDateString("en-GB", {
    weekday: "short",
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
  return `${day}, ${time}`;
}

/** "42 min", "1 h 05 min". */
export function duration(ms: number): string {
  const minutes = Math.max(1, Math.round(ms / MINUTE));
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m ? `${h} h ${String(m).padStart(2, "0")} min` : `${h} h`;
}

/** "12:04" or "1:02:09" for timers. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h ? String(m).padStart(2, "0") : String(m);
  return `${h ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** "Untitled" for empty titles. */
export function displayTitle(title: string | null | undefined): string {
  return title?.trim() ? title.trim() : "Untitled";
}

/** Currencies by region, for the swear jar's pretend price. Anything else is shown in dollars. */
const REGION_CURRENCY: Record<string, string> = {
  GB: "GBP",
  US: "USD",
  CA: "CAD",
  AU: "AUD",
  NZ: "NZD",
  IE: "EUR",
  DE: "EUR",
  FR: "EUR",
  ES: "EUR",
  IT: "EUR",
  NL: "EUR",
  BE: "EUR",
  AT: "EUR",
  PT: "EUR",
  FI: "EUR",
  CH: "CHF",
  SE: "SEK",
  NO: "NOK",
  DK: "DKK",
  IN: "INR",
  ZA: "ZAR",
  SG: "SGD",
};

/** The currency of the region in `locale` ("en-GB" is pounds). */
export function localCurrency(locale: string = navigator.language): string {
  try {
    const region = new Intl.Locale(locale).maximize().region ?? "";
    return REGION_CURRENCY[region] ?? "USD";
  } catch {
    return "USD";
  }
}

/** Hundredths of a currency as money: "£1", "£0.50", "£23". */
export function money(cents: number, currency: string = localCurrency(), locale?: string): string {
  const whole = cents % 100 === 0;
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency,
    minimumFractionDigits: whole ? 0 : 2,
    maximumFractionDigits: 2,
  }).format(cents / 100);
}
