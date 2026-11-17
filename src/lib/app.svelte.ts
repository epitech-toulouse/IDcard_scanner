// État global de l'interface : route, statut du lecteur, notifications, dialogues.

import { ask, open, save } from "@tauri-apps/plugin-dialog";
import type { ReaderStatus } from "./types";

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

export const reader = $state<ReaderStatus>({ connected: false, backend: "none", message: "Recherche du lecteur…" });

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
