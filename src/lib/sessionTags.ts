// Repères A, B, C… des créneaux ouverts en même temps (écran d'émargement, projection, borne).
// L'ordre est celui de `sessions::open_rows` côté Rust : début, puis id.

import type { Id, SessionRow } from "./types";

/** Nombre de couleurs de repère (classes `.tag-0` … dans app.css, mêmes teintes sur la borne). */
export const TAG_COLORS = 4;

/** Créneaux ouverts dans l'ordre de leurs repères. */
export function openInTagOrder(rows: SessionRow[]): SessionRow[] {
  return rows
    .filter((r) => r.state === "open")
    .sort((a, b) => (a.starts_at < b.starts_at ? -1 : a.starts_at > b.starts_at ? 1 : a.id - b.id));
}

export interface SessionTag {
  /** A, B, C… ; « · » pour un créneau qui n'est pas ouvert. */
  letter: string;
  /** Classe CSS de la couleur. */
  cls: string;
}

/** Lettre au-delà de Z : AA, AB… (cas d'école, mais jamais deux fois la même). */
export function tagLetter(i: number): string {
  let s = "";
  for (let n = i; n >= 0; n = Math.floor(n / 26) - 1) s = String.fromCharCode(65 + (n % 26)) + s;
  return s;
}

export function tagFor(open: SessionRow[], id: Id | null): SessionTag {
  const i = id === null ? -1 : open.findIndex((r) => r.id === id);
  return i < 0 ? { letter: "·", cls: "tag-none" } : { letter: tagLetter(i), cls: `tag-${i % TAG_COLORS}` };
}
