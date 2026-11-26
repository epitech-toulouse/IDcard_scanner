// Wrappers typés des commandes Tauri (src-tauri/src/commands.rs).
// Arguments de commande en camelCase ; champs des objets en snake_case (serde).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "./types";

export const api = {
  // Groupes
  tree: () => invoke<T.ProgramNode[]>("tree"),
  createProgram: (name: string) => invoke<T.Program>("create_program", { name }),
  createCohort: (programId: T.Id, name: string, schoolYear: string) =>
    invoke<T.Cohort>("create_cohort", { programId, name, schoolYear }),
  createGroup: (cohortId: T.Id, name: string) => invoke<T.Group>("create_group", { cohortId, name }),
  renameNode: (kind: T.NodeKind, id: T.Id, name: string) => invoke<void>("rename_node", { kind, id, name }),
  setSchoolYear: (cohortId: T.Id, schoolYear: string) => invoke<void>("set_school_year", { cohortId, schoolYear }),
  deleteNode: (kind: T.NodeKind, id: T.Id) => invoke<void>("delete_node", { kind, id }),

  // Étudiants
  listStudents: (filter: T.StudentFilter = {}) => invoke<T.StudentRow[]>("list_students", { filter }),
  createStudent: (input: T.StudentInput, groupIds: T.Id[]) => invoke<T.Student>("create_student", { input, groupIds }),
  updateStudent: (id: T.Id, input: T.StudentInput, groupIds: T.Id[]) =>
    invoke<T.Student>("update_student", { id, input, groupIds }),
  deleteStudent: (id: T.Id) => invoke<void>("delete_student", { id }),
  bulkStudents: (ids: T.Id[], action: T.BulkAction) => invoke<number>("bulk_students", { ids, action }),
  linkCard: (uid: string, studentId: T.Id) => invoke<void>("link_card", { uid, studentId }),
  unlinkCard: (uid: string) => invoke<void>("unlink_card", { uid }),

  // Import
  importPreview: (path: string) => invoke<T.Preview>("import_preview", { path }),
  importRun: (path: string, mapping: T.ColumnMapping, cohortId: T.Id) =>
    invoke<T.ImportReport>("import_run", { path, mapping, cohortId }),

  // Créneaux
  listSessions: (from: T.Naive | null = null, to: T.Naive | null = null) =>
    invoke<T.SessionRow[]>("list_sessions", { from, to }),
  sessionDetail: (id: T.Id) => invoke<T.SessionDetail>("session_detail", { id }),
  createSession: (input: T.SessionInput) => invoke<T.Session>("create_session", { input }),
  updateSession: (id: T.Id, input: T.SessionInput) => invoke<T.Session>("update_session", { id, input }),
  duplicateSession: (id: T.Id, startsAt: T.Naive) => invoke<T.Session>("duplicate_session", { id, startsAt }),
  deleteSession: (id: T.Id) => invoke<void>("delete_session", { id }),
  openSession: (id: T.Id) => invoke<void>("open_session", { id }),
  closeSession: (id: T.Id) => invoke<number>("close_session", { id }),
  reopenSession: (id: T.Id) => invoke<void>("reopen_session", { id }),
  getActiveSession: () => invoke<T.Id | null>("get_active_session"),
  setActiveSession: (id: T.Id | null) => invoke<void>("set_active_session", { id }),
  setAttendance: (sessionId: T.Id, studentId: T.Id, status: T.Status, reason: string | null = null) =>
    invoke<void>("set_attendance", { sessionId, studentId, status, reason }),
  clearAttendance: (sessionId: T.Id, studentId: T.Id) => invoke<void>("clear_attendance", { sessionId, studentId }),
  acceptUnexpected: (sessionId: T.Id, studentId: T.Id) =>
    invoke<T.Status>("accept_unexpected", { sessionId, studentId }),
  rescan: (uid: string) => invoke<T.ScanOutcome>("rescan", { uid }),

  // Export
  exportSession: (sessionId: T.Id, path: string) => invoke<void>("export_session", { sessionId, path }),
  exportPeriod: (filter: T.PeriodFilter, path: string) => invoke<T.ExportSummary>("export_period", { filter, path }),

  // Lecteur
  readerStatus: () => invoke<T.ReadersStatus>("reader_status"),
  injectCard: (uid: string) => invoke<void>("inject_card", { uid }),
  setCardTest: (enabled: boolean) => invoke<void>("set_card_test", { enabled }),

  // Réglages
  getSettings: () => invoke<T.Settings>("get_settings"),
  saveSettings: (settings: T.Settings) => invoke<void>("save_settings", { settings }),
  backupDb: (path: string) => invoke<void>("backup_db", { path }),
  restoreDb: (path: string) => invoke<void>("restore_db", { path }),
  dataPath: () => invoke<string>("data_path"),
  updateInfo: () => invoke<T.UpdateInfo>("update_info"),
  updateGenerateKeys: (regenerate: boolean) => invoke<T.UpdateInfo>("update_generate_keys", { regenerate }),
  updatePublish: (path: string) => invoke<T.UpdateInfo>("update_publish", { path }),
  updatePublishBundled: () => invoke<T.UpdateInfo>("update_publish_bundled"),
  updateUnpublish: () => invoke<T.UpdateInfo>("update_unpublish"),
  localAddress: () => invoke<string | null>("local_address"),
};

export const events = {
  onCard: (cb: (o: T.ScanOutcome) => void): Promise<UnlistenFn> =>
    listen<T.ScanOutcome>("card-scanned", (e) => cb(e.payload)),
  onReader: (cb: (s: T.ReadersStatus) => void): Promise<UnlistenFn> =>
    listen<T.ReadersStatus>("reader-status", (e) => cb(e.payload)),
  onCardTest: (cb: (d: T.CardDiagnostic) => void): Promise<UnlistenFn> =>
    listen<T.CardDiagnostic>("card-test", (e) => cb(e.payload)),
  onSessionAuto: (cb: (e: T.AutoEvent) => void): Promise<UnlistenFn> =>
    listen<T.AutoEvent>("session-auto", (e) => cb(e.payload)),
  /** Le créneau actif a changé tout seul (le précédent est terminé, un autre est en cours). */
  onActiveSession: (cb: (id: T.Id | null) => void): Promise<UnlistenFn> =>
    listen<T.Id | null>("active-session", (e) => cb(e.payload)),
  onError: (cb: (msg: string) => void): Promise<UnlistenFn> => listen<string>("app-error", (e) => cb(e.payload)),
};
