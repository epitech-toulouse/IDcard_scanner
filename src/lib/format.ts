// Dates « naïves » (heure locale, sans fuseau) échangées avec le backend : "YYYY-MM-DDTHH:MM:SS".

import type { Naive } from "./types";

const pad = (n: number) => String(n).padStart(2, "0");

export function toNaive(d: Date): Naive {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function parseNaive(s: Naive): Date {
  const [date, time = "00:00:00"] = s.split("T");
  const [y, m, d] = date.split("-").map(Number);
  const [hh, mm, ss = 0] = time.split(":").map(Number);
  return new Date(y, m - 1, d, hh, mm, ss);
}

/** Combine un champ <input type="date"> et un <input type="time">. */
export function joinNaive(date: string, time: string): Naive {
  return `${date}T${time.length === 5 ? time + ":00" : time}`;
}

export function splitNaive(s: Naive): { date: string; time: string } {
  const [date, time = "00:00:00"] = s.split("T");
  return { date, time: time.slice(0, 5) };
}

export function addMinutes(s: Naive, min: number): Naive {
  const d = parseNaive(s);
  d.setMinutes(d.getMinutes() + min);
  return toNaive(d);
}

export function addDays(s: Naive, days: number): Naive {
  const d = parseNaive(s);
  d.setDate(d.getDate() + days);
  return toNaive(d);
}

export function startOfDay(d: Date): Naive {
  return toNaive(new Date(d.getFullYear(), d.getMonth(), d.getDate()));
}

export function endOfDay(d: Date): Naive {
  return toNaive(new Date(d.getFullYear(), d.getMonth(), d.getDate(), 23, 59, 59));
}

/** Lundi 00:00 de la semaine de `d`. */
export function startOfWeek(d: Date): Naive {
  const day = (d.getDay() + 6) % 7; // lundi = 0
  return toNaive(new Date(d.getFullYear(), d.getMonth(), d.getDate() - day));
}

/** Année scolaire courante : « 2026-2027 » à partir de septembre. */
export function schoolYear(d: Date = new Date()): string {
  const y = d.getMonth() >= 8 ? d.getFullYear() : d.getFullYear() - 1;
  return `${y}-${y + 1}`;
}

const dateFmt = new Intl.DateTimeFormat("fr-FR", { weekday: "short", day: "2-digit", month: "2-digit", year: "numeric" });

export function fmtDate(s: Naive): string {
  return dateFmt.format(parseNaive(s));
}

export function fmtTime(s: Naive | null): string {
  return s ? splitNaive(s).time.replace(":", "h") : "";
}

export function fmtRange(start: Naive, end: Naive): string {
  return `${fmtDate(start)} · ${fmtTime(start)} – ${fmtTime(end)}`;
}

export function isToday(s: Naive, now = new Date()): boolean {
  return s.slice(0, 10) === toNaive(now).slice(0, 10);
}

/** Taux en pourcentage arrondi, « – » si rien d'attendu. */
export function pct(n: number, total: number): string {
  return total > 0 ? `${Math.round((100 * n) / total)} %` : "–";
}

/** Nom de fichier sûr : « Réseaux 17-11-2026 » → « Reseaux_17-11-2026 ». */
export function safeFileName(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^A-Za-z0-9._-]+/g, "_")
    .replace(/^_+|_+$/g, "");
}
