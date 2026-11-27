// Placement des créneaux dans la vue semaine (règles pures, testées dans calendar.test.ts).

import type { Session } from "./types";

const hourOf = (s: string) => {
  const [h, m] = s.slice(11, 16).split(":").map(Number);
  return h + m / 60;
};

/** Plage horaire affichée : 8 h – 18 h au minimum, élargie aux créneaux de la semaine (heures entières). */
export function weekHours(rows: Pick<Session, "starts_at" | "ends_at">[]): { from: number; to: number } {
  let from = 8, to = 18;
  for (const r of rows) {
    from = Math.min(from, Math.floor(hourOf(r.starts_at)));
    const sameDay = r.ends_at.slice(0, 10) === r.starts_at.slice(0, 10);
    to = Math.max(to, sameDay ? Math.ceil(hourOf(r.ends_at)) : 24);
  }
  return { from, to: Math.min(24, Math.max(to, from + 1)) };
}

export interface Placed<T> {
  row: T;
  top: number;
  height: number;
  /** Position et largeur horizontales, en % de la colonne du jour. */
  left: number;
  width: number;
}

/**
 * Place les créneaux d'un même jour : les créneaux qui se chevauchent se partagent la largeur
 * de la colonne (une piste chacun), les autres l'occupent entièrement.
 */
export function layoutDay<T extends Pick<Session, "starts_at" | "ends_at">>(rows: T[], fromHour: number, hourPx: number): Placed<T>[] {
  const sorted = [...rows].sort((a, b) => a.starts_at.localeCompare(b.starts_at) || b.ends_at.localeCompare(a.ends_at));
  const out: Placed<T>[] = [];
  let cluster: { item: Placed<T>; lane: number; end: string }[] = [];
  let clusterEnd = "";
  const flush = () => {
    const lanes = Math.max(1, ...cluster.map((c) => c.lane + 1));
    for (const c of cluster) {
      c.item.width = 100 / lanes;
      c.item.left = c.lane * c.item.width;
    }
    cluster = [];
  };
  for (const r of sorted) {
    if (cluster.length && r.starts_at >= clusterEnd) flush();
    const laneEnds: string[] = [];
    for (const c of cluster) laneEnds[c.lane] = laneEnds[c.lane] && laneEnds[c.lane] > c.end ? laneEnds[c.lane] : c.end;
    let lane = laneEnds.findIndex((e) => e <= r.starts_at);
    if (lane < 0) lane = laneEnds.length;
    const sameDay = r.ends_at.slice(0, 10) === r.starts_at.slice(0, 10);
    const start = hourOf(r.starts_at), end = sameDay ? hourOf(r.ends_at) : 24;
    const item: Placed<T> = {
      row: r,
      top: (start - fromHour) * hourPx + 2,
      height: Math.max(22, (end - start) * hourPx - 4),
      left: 0,
      width: 100,
    };
    if (!cluster.length) clusterEnd = r.ends_at;
    else if (r.ends_at > clusterEnd) clusterEnd = r.ends_at;
    cluster.push({ item, lane, end: r.ends_at });
    out.push(item);
  }
  flush();
  return out;
}
