// Miroir des types de crates/badgeuse-core/src/model.rs (à garder synchronisé).
// Les dates sont des NaiveDateTime chrono : "2026-11-17T08:00:00", heure locale.

export type Id = number;
export type Naive = string;

export type Status = "present" | "late" | "absent" | "excused";
export type Source = "badge" | "manual" | "auto";
export type SessionState = "planned" | "open" | "closed";
export type NodeKind = "program" | "cohort" | "group";

export interface Program { id: Id; name: string }
export interface Cohort { id: Id; program_id: Id; name: string; school_year: string }
export interface Group { id: Id; cohort_id: Id; name: string }
export interface GroupNode extends Group { student_count: number }
export interface CohortNode extends Cohort { student_count: number; groups: GroupNode[] }
export interface ProgramNode extends Program { cohorts: CohortNode[] }

export interface Student {
  id: Id;
  student_number: string;
  last_name: string;
  first_name: string;
  email: string | null;
  cohort_id: Id | null;
  active: boolean;
}
export type StudentInput = Omit<Student, "id">;
export interface StudentRow extends Student {
  cohort_label: string | null;
  group_ids: Id[];
  groups: string;
  cards: string[];
}
export interface StudentFilter {
  cohort_id?: Id | null;
  group_id?: Id | null;
  search?: string | null;
  include_inactive?: boolean;
}

export interface Session {
  id: Id;
  title: string;
  room: string | null;
  starts_at: Naive;
  ends_at: Naive;
  late_after_min: number;
  state: SessionState;
}
export interface SessionInput {
  title: string;
  room: string | null;
  starts_at: Naive;
  ends_at: Naive;
  late_after_min: number;
  group_ids: Id[];
  cohort_ids: Id[];
}
export interface Counts { expected: number; present: number; late: number; absent: number; excused: number }
export interface SessionRow extends Session {
  group_ids: Id[];
  cohort_ids: Id[];
  targets: string;
  counts: Counts;
}
export interface AttendanceRow {
  student: Student;
  groups: string;
  expected: boolean;
  status: Status | null;
  scanned_at: Naive | null;
  source: Source | null;
  reason: string | null;
}
export interface SessionDetail { session: SessionRow; rows: AttendanceRow[] }

export type ScanOutcome =
  | { kind: "recorded"; uid: string; student: Student; status: Status; scanned_at: Naive }
  | { kind: "already_scanned"; uid: string; student: Student; status: Status }
  | { kind: "not_expected"; uid: string; student: Student }
  | { kind: "unknown_card"; uid: string }
  | { kind: "identified"; uid: string; student: Student | null }
  | { kind: "session_not_open"; uid: string }
  | { kind: "random_uid"; uid: string };

export interface ReaderStatus { name: string; connected: boolean; backend: "pcsc" | "usb" | "mock" | "none"; message: string }
export interface ReadersStatus {
  connected: boolean;
  backend: "pcsc" | "usb" | "mock" | "none";
  message: string;
  count: number;
  connected_count: number;
  readers: ReaderStatus[];
}

export interface ColumnMapping {
  student_number: number | null;
  last_name: number | null;
  first_name: number | null;
  email: number | null;
  group: number | null;
}
export interface Preview { headers: string[]; sample: string[][]; total_rows: number; mapping: ColumnMapping }
export interface ImportReport { created: number; updated: number; groups_created: number; skipped: number; errors: string[] }

export interface PeriodFilter { from: Naive | null; to: Naive | null; group_ids: Id[]; cohort_ids: Id[] }
export interface ExportSummary { sessions: number; students: number }

export interface Settings {
  default_late_min: number;
  default_duration_min: number;
  reader: "auto" | "pcsc" | "usb" | "mock";
  reader_feedback: boolean;
}

export const STATUS_LABEL: Record<Status, string> = {
  present: "Présent",
  late: "En retard",
  absent: "Absent",
  excused: "Absence justifiée",
};
export const SOURCE_LABEL: Record<Source, string> = { badge: "Badge", manual: "Manuel", auto: "Clôture" };
export const STATE_LABEL: Record<SessionState, string> = { planned: "Prévu", open: "En cours", closed: "Clos" };
