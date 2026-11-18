// Message affiché pour un badge (écran d'émargement, projection, affichage automatique).

import { fmtTimeSec } from "./format";
import { STATUS_LABEL, type ScanOutcome, type Student } from "./types";

export interface ScanMessage {
  /** Classe CSS : present | late | absent | excused. */
  cls: string;
  title: string;
  sub: string;
}

export const studentName = (st: Student) => `${st.first_name} ${st.last_name.toUpperCase()}`;

export function describeScan(o: ScanOutcome): ScanMessage {
  switch (o.kind) {
    case "recorded":
      return {
        cls: o.status,
        title: studentName(o.student),
        sub: `${o.status === "late" ? "En retard" : "Présent"} · ${fmtTimeSec(o.scanned_at)}`,
      };
    case "already_scanned":
      return { cls: o.status, title: studentName(o.student), sub: `Déjà enregistré (${STATUS_LABEL[o.status].toLowerCase()})` };
    case "not_expected":
      return { cls: "absent", title: studentName(o.student), sub: "N'est pas attendu sur ce créneau" };
    case "unknown_card":
      return { cls: "absent", title: "Carte inconnue", sub: o.uid };
    case "random_uid":
      return { cls: "absent", title: "Carte non utilisable", sub: "UID aléatoire (téléphone, carte bancaire…)" };
    case "session_not_open":
      return { cls: "absent", title: "Émargement fermé", sub: "Ouvrez le créneau pour enregistrer les badges" };
    case "identified":
      return {
        cls: o.student ? "present" : "absent",
        title: o.student ? studentName(o.student) : "Carte inconnue",
        sub: "Aucun créneau ouvert",
      };
  }
}
