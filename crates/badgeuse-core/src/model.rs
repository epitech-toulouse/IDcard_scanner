//! Types partagés, sérialisés tels quels vers le front (voir `src/lib/types.ts`).

use chrono::NaiveDateTime;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

pub type Id = i64;

macro_rules! sql_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $s),+ }
            }
        }
        impl std::str::FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s { $($s => Ok(Self::$variant),)+ _ => Err(format!("valeur inconnue : {s}")) }
            }
        }
        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }
        impl FromSql for $name {
            fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
                v.as_str()?.parse().map_err(|e: String| FromSqlError::Other(e.into()))
            }
        }
    };
}

sql_enum!(Status { Present => "present", Late => "late", Absent => "absent", Excused => "excused" });
sql_enum!(Source { Badge => "badge", Manual => "manual", Auto => "auto" });
sql_enum!(SessionState { Planned => "planned", Open => "open", Closed => "closed" });

impl Status {
    /// Code court utilisé dans les exports Excel.
    pub fn code(self) -> &'static str {
        match self {
            Status::Present => "P",
            Status::Late => "R",
            Status::Absent => "A",
            Status::Excused => "AJ",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Status::Present => "Présent",
            Status::Late => "En retard",
            Status::Absent => "Absent",
            Status::Excused => "Absence justifiée",
        }
    }
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Badge => "Badge",
            Source::Manual => "Manuel",
            Source::Auto => "Clôture",
        }
    }
}

// ---------- Groupes ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub id: Id,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cohort {
    pub id: Id,
    pub program_id: Id,
    pub name: String,
    pub school_year: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Id,
    pub cohort_id: Id,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupNode {
    #[serde(flatten)]
    pub group: Group,
    pub student_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CohortNode {
    #[serde(flatten)]
    pub cohort: Cohort,
    pub student_count: i64,
    pub groups: Vec<GroupNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgramNode {
    #[serde(flatten)]
    pub program: Program,
    pub cohorts: Vec<CohortNode>,
}

// ---------- Étudiants ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Student {
    pub id: Id,
    pub student_number: String,
    pub last_name: String,
    pub first_name: String,
    pub email: Option<String>,
    pub cohort_id: Option<Id>,
    pub active: bool,
}

impl Student {
    pub fn full_name(&self) -> String {
        format!("{} {}", self.last_name.to_uppercase(), self.first_name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StudentInput {
    pub student_number: String,
    pub last_name: String,
    pub first_name: String,
    pub email: Option<String>,
    pub cohort_id: Option<Id>,
    #[serde(default = "yes")]
    pub active: bool,
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct StudentRow {
    #[serde(flatten)]
    pub student: Student,
    pub cohort_label: Option<String>,
    pub group_ids: Vec<Id>,
    pub groups: String,
    pub cards: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StudentFilter {
    pub cohort_id: Option<Id>,
    pub group_id: Option<Id>,
    pub search: Option<String>,
    #[serde(default)]
    pub include_inactive: bool,
}

// ---------- Créneaux ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Id,
    pub title: String,
    pub room: Option<String>,
    pub starts_at: NaiveDateTime,
    pub ends_at: NaiveDateTime,
    pub late_after_min: i64,
    pub state: SessionState,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionInput {
    pub title: String,
    pub room: Option<String>,
    pub starts_at: NaiveDateTime,
    pub ends_at: NaiveDateTime,
    pub late_after_min: i64,
    #[serde(default)]
    pub group_ids: Vec<Id>,
    #[serde(default)]
    pub cohort_ids: Vec<Id>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub expected: i64,
    pub present: i64,
    pub late: i64,
    pub absent: i64,
    pub excused: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRow {
    #[serde(flatten)]
    pub session: Session,
    pub group_ids: Vec<Id>,
    pub cohort_ids: Vec<Id>,
    pub targets: String,
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttendanceRow {
    pub student: Student,
    pub groups: String,
    pub expected: bool,
    pub status: Option<Status>,
    pub scanned_at: Option<NaiveDateTime>,
    pub source: Option<Source>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionDetail {
    pub session: SessionRow,
    pub rows: Vec<AttendanceRow>,
}
