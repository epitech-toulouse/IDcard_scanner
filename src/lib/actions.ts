// Actions partagées entre pages.

import { api } from "./api";
import { guard, saveFile, toast } from "./app.svelte";
import { safeFileName } from "./format";
import type { Session } from "./types";

export async function exportSessionXlsx(s: Session) {
  const name = safeFileName(`Emargement_${s.title}_${s.starts_at.slice(0, 10)}`) + ".xlsx";
  const path = await saveFile(name);
  if (!path) return;
  if ((await guard(api.exportSession(s.id, path))) !== undefined) toast(`Export enregistré : ${path}`, "success", 6000);
}
