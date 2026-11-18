// Actions partagées entre pages.

import { api } from "./api";
import { EXPORT_FILTERS, guard, saveFile, toast, type ExportFormat } from "./app.svelte";
import { safeFileName } from "./format";
import type { Session } from "./types";

/** Exporte la feuille d'un créneau ; le core choisit le format d'après l'extension. */
export async function exportSession(s: Session, format: ExportFormat = "xlsx") {
  const name = safeFileName(`Emargement_${s.title}_${s.starts_at.slice(0, 10)}`) + "." + format;
  const path = await saveFile(name, EXPORT_FILTERS[format]);
  if (!path) return;
  if ((await guard(api.exportSession(s.id, path))) !== undefined) toast(`Export enregistré : ${path}`, "success", 6000);
}
