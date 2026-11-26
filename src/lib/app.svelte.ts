// État global de l'interface : route, statut du lecteur, notifications, dialogues.

import { ask, open, save } from "@tauri-apps/plugin-dialog";
import type { AppUpdate, AppUpdateProgress, ReadersStatus } from "./types";
import { api } from "./api";

// ---------- Routage par hash : #/emargement/12 ----------

function parseHash(): string[] {
  const parts = location.hash.replace(/^#\/?/, "").split("/").filter(Boolean);
  return parts.length ? parts : ["accueil"];
}

export const route = $state({ parts: parseHash() });
window.addEventListener("hashchange", () => (route.parts = parseHash()));

export function navigate(path: string) {
  location.hash = "#/" + path.replace(/^\//, "");
}

// ---------- Lecteur ----------

export const reader = $state<ReadersStatus>({
  connected: false,
  backend: "none",
  message: "Recherche du lecteur…",
  count: 0,
  connected_count: 0,
  readers: [],
});

// ---------- Notifications ----------

export type ToastKind = "info" | "success" | "warn" | "error";
export interface Toast { id: number; kind: ToastKind; text: string }
export const toasts = $state<Toast[]>([]);
let nextId = 1;

export function toast(text: string, kind: ToastKind = "info", ms = 4500) {
  const id = nextId++;
  toasts.push({ id, kind, text });
  setTimeout(() => dismiss(id), ms);
}

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

/** Exécute une promesse et affiche l'erreur éventuelle (les erreurs Rust arrivent en string). */
export async function guard<T>(p: Promise<T>): Promise<T | undefined> {
  try {
    return await p;
  } catch (e) {
    toast(typeof e === "string" ? e : (e as Error)?.message ?? String(e), "error", 7000);
    return undefined;
  }
}

// ---------- Dialogues natifs ----------

export const SPREADSHEETS = [{ name: "Tableur (CSV, Excel, ODS)", extensions: ["csv", "xlsx", "xls", "ods", "txt", "tsv"] }];
export const XLSX = [{ name: "Classeur Excel", extensions: ["xlsx"] }];
export const CSV = [{ name: "CSV (séparateur ;)", extensions: ["csv"] }];
export type ExportFormat = "xlsx" | "csv";
export const EXPORT_FILTERS: Record<ExportFormat, typeof XLSX> = { xlsx: XLSX, csv: CSV };
export const DB = [{ name: "Base Badgeuse", extensions: ["db", "sqlite"] }];

export async function pickFile(filters = SPREADSHEETS): Promise<string | null> {
  const p = await open({ multiple: false, directory: false, filters });
  return typeof p === "string" ? p : null;
}

export async function saveFile(defaultPath: string, filters = XLSX): Promise<string | null> {
  return (await save({ defaultPath, filters })) ?? null;
}

export async function confirmAction(message: string, title = "Confirmation"): Promise<boolean> {
  return ask(message, { title, kind: "warning", okLabel: "Confirmer", cancelLabel: "Annuler" });
}

// ---- Mise à jour de l'appli ----

export const appUpdate = $state<{ info: AppUpdate | null; dismissed: boolean; installing: boolean; progress: AppUpdateProgress | null }>({
  info: null,
  dismissed: false,
  installing: false,
  progress: null,
});

/** Télécharge et installe la nouvelle version, puis l'appli redémarre d'elle-même. */
export async function installAppUpdate() {
  if (!appUpdate.info || appUpdate.installing) return;
  const ok = await confirmAction(
    `Installer Badgeuse ${appUpdate.info.version} ?\n\nL'appli redémarre à la fin : les badges passés pendant l'installation (environ une minute) ne sont pas enregistrés. Le mot de passe administrateur peut être demandé.`,
    "Mise à jour de Badgeuse",
  );
  if (!ok) return;
  appUpdate.installing = true;
  appUpdate.progress = null;
  try {
    await api.appUpdateInstall();
  } catch (e) {
    toast(String(e), "error", 9000);
  } finally {
    appUpdate.installing = false;
  }
}
