// Règles de l'affichage automatique (page Affichage) : quand projeter un créneau, quel est le suivant.
import { addMinutes } from "./format";
import type { Naive, Session } from "./types";

/** Fin de la projection d'un créneau : début + tolérance de retard, sans dépasser la fin du créneau. */
export function projectionEnd(s: Session): Naive {
  const end = addMinutes(s.starts_at, Math.max(0, s.late_after_min));
  return end < s.ends_at ? end : s.ends_at;
}

/** Un créneau ouvert est projeté jusqu'à la fin de sa tolérance de retard ; ensuite, écran d'attente. */
export function isProjected(s: Session, now: Naive): boolean {
  return s.state === "open" && now < projectionEnd(s);
}

/** Prochain créneau à annoncer : non clos et dont la fenêtre de projection n'est pas passée. */
export function nextSession<T extends Session>(list: T[], now: Naive, exclude: number | null = null): T | null {
  return (
    list
      .filter((r) => r.id !== exclude && r.state !== "closed" && now < projectionEnd(r))
      .sort((a, b) => a.starts_at.localeCompare(b.starts_at))[0] ?? null
  );
}

/** Heure annoncée pour le prochain émargement : ouverture automatique si activée, sinon le début. */
export function nextOpeningAt(s: Session, autoOpenLeadMin: number | null): Naive {
  if (autoOpenLeadMin === null) return s.starts_at;
  return addMinutes(s.starts_at, -Math.max(0, autoOpenLeadMin));
}
