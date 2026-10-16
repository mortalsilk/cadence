use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
const MAX_DURATION: u64 = 315_360_000_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    CountUp,
    Countdown,
    Manual,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Running,
    Paused,
    Review,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stint {
    pub id: String,
    pub label: String,
    pub description: String,
    pub mode: Mode,
    pub target_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub started_at: String,
    pub finished_at: String,
    pub date: String,
    #[serde(default)]
    pub session_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub label: String,
    pub description: String,
    pub elapsed_ms: u64,
    pub started_at: String,
    pub finished_at: String,
    pub date: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Active {
    pub id: String,
    pub label: String,
    pub mode: Mode,
    pub target_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub date: Option<String>,
    pub phase: Phase,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(skip)]
    pub anchor_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub id: String,
    pub label: String,
    pub description: String,
    pub elapsed_ms: u64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub date: Option<String>,
    pub phase: Phase,
    #[serde(skip)]
    pub anchor_ms: u64,
}
#[derive(Debug, Clone)]
pub struct Stamp {
    pub mono_ms: u64,
    pub wall: DateTime<FixedOffset>,
}
impl Stamp {
    pub fn iso(&self) -> String {
        self.wall.to_rfc3339()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInput {
    pub label: String,
    pub description: String,
    pub date: String,
    pub elapsed_ms: u64,
    #[serde(default)]
    pub session_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInput {
    pub label: String,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backup {
    pub version: u32,
    #[serde(default)]
    pub sessions: Vec<Session>,
    pub stints: Vec<Stint>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub total: usize,
    pub added: usize,
    pub skipped: usize,
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct DaySummary {
    pub count: u64,
    pub session_count: u64,
    pub elapsed_ms: u64,
}

pub struct Engine {
    db: Connection,
    pub active: Option<Active>,
    pub active_session: Option<ActiveSession>,
}

impl Engine {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path).map_err(err)?)
    }
    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(err)?)
    }
    fn from_connection(db: Connection) -> Result<Self> {
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(err)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
            .map_err(err)?;
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(err)?;
        if version > 2 {
            return Err("This database was created by a newer cadence version.".into());
        }
        if version == 0 {
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE stints (id TEXT PRIMARY KEY, date TEXT NOT NULL, data TEXT NOT NULL);
                CREATE INDEX stints_date ON stints(date);
                CREATE TABLE active (singleton INTEGER PRIMARY KEY CHECK(singleton=1), data TEXT NOT NULL);
                CREATE TABLE sessions (id TEXT PRIMARY KEY, date TEXT NOT NULL, data TEXT NOT NULL);
                CREATE INDEX sessions_date ON sessions(date);
                CREATE TABLE active_session (singleton INTEGER PRIMARY KEY CHECK(singleton=1), data TEXT NOT NULL);
                PRAGMA user_version=2; COMMIT;").map_err(err)?;
        } else if version == 1 {
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE sessions (id TEXT PRIMARY KEY, date TEXT NOT NULL, data TEXT NOT NULL);
                CREATE INDEX sessions_date ON sessions(date);
                CREATE TABLE active_session (singleton INTEGER PRIMARY KEY CHECK(singleton=1), data TEXT NOT NULL);
                PRAGMA user_version=2; COMMIT;").map_err(err)?;
        }
        let active = read_singleton::<Active>(&db, "active")?;
        let active_session = read_singleton::<ActiveSession>(&db, "active_session")?;
        let mut engine = Self {
            db,
            active,
            active_session,
        };
        if let Some(a) = &mut engine.active {
            if a.phase == Phase::Running {
                a.phase = Phase::Paused;
            }
        }
        if let Some(s) = &mut engine.active_session {
            if s.phase == Phase::Running {
                s.phase = Phase::Paused;
            }
        }
        engine.persist_active()?;
        engine.persist_active_session()?;
        Ok(engine)
    }
    fn persist_active(&self) -> Result<()> {
        persist_singleton(&self.db, "active", &self.active)
    }
    fn persist_active_session(&self) -> Result<()> {
        persist_singleton(&self.db, "active_session", &self.active_session)
    }
    fn set_active(&mut self, value: Option<Active>) -> Result<()> {
        persist_singleton(&self.db, "active", &value)?;
        self.active = value;
        Ok(())
    }
    fn set_active_session(&mut self, value: Option<ActiveSession>) -> Result<()> {
        persist_singleton(&self.db, "active_session", &value)?;
        self.active_session = value;
        Ok(())
    }
    pub fn snapshot(&self, now: &Stamp) -> Option<Active> {
        self.active.clone().map(|mut a| {
            if a.phase == Phase::Running {
                a.elapsed_ms = a
                    .elapsed_ms
                    .saturating_add(now.mono_ms.saturating_sub(a.anchor_ms))
                    .min(MAX_DURATION);
            }
            if let Some(target) = a.target_ms {
                a.elapsed_ms = a.elapsed_ms.min(target);
            }
            a.anchor_ms = now.mono_ms;
            a
        })
    }
    pub fn session_snapshot(&self, now: &Stamp) -> Option<ActiveSession> {
        self.active_session.clone().map(|mut s| {
            if s.phase == Phase::Running {
                s.elapsed_ms = s
                    .elapsed_ms
                    .saturating_add(now.mono_ms.saturating_sub(s.anchor_ms))
                    .min(MAX_DURATION);
            }
            s.anchor_ms = now.mono_ms;
            s
        })
    }

    pub fn start_session(&mut self, input: SessionInput, now: &Stamp) -> Result<()> {
        if self.active_session.is_some() {
            return Err("End or discard the current session first.".into());
        }
        if self.active.is_some() {
            return Err("Resolve the active stint before starting a session.".into());
        }
        validate_text(&input.label, &input.description, "session")?;
        self.set_active_session(Some(ActiveSession {
            id: Uuid::new_v4().to_string(),
            label: input.label.trim().into(),
            description: input.description.trim().into(),
            elapsed_ms: 0,
            started_at: now.iso(),
            finished_at: None,
            date: None,
            phase: Phase::Running,
            anchor_ms: now.mono_ms,
        }))
    }
    pub fn pause_session(&mut self, now: &Stamp) -> Result<()> {
        self.tick(now)?;
        if self
            .active
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Running)
        {
            self.pause(now)?;
        }
        let mut s = self.session_snapshot(now).ok_or("No active session.")?;
        if s.phase == Phase::Running {
            s.phase = Phase::Paused;
            self.set_active_session(Some(s))?;
        }
        Ok(())
    }
    pub fn resume_session(&mut self, now: &Stamp) -> Result<()> {
        let mut s = self.active_session.clone().ok_or("No active session.")?;
        if s.phase == Phase::Review {
            return Err("Save or discard the ended session.".into());
        }
        if s.phase == Phase::Paused {
            s.phase = Phase::Running;
            s.anchor_ms = now.mono_ms;
            self.set_active_session(Some(s))?;
        }
        Ok(())
    }
    pub fn end_session(&mut self, now: &Stamp) -> Result<()> {
        if self.active.is_some() {
            return Err("Save or discard the active stint before ending the session.".into());
        }
        let mut s = self.session_snapshot(now).ok_or("No active session.")?;
        if s.phase != Phase::Review {
            s.phase = Phase::Review;
            s.finished_at = Some(now.iso());
            s.date = Some(now.wall.date_naive().to_string());
            self.set_active_session(Some(s))?;
        }
        Ok(())
    }
    pub fn save_session(&mut self, input: SessionInput) -> Result<Session> {
        let a = self.active_session.as_ref().ok_or("No session to save.")?;
        if a.phase != Phase::Review {
            return Err("End the session before saving it.".into());
        }
        validate_text(&input.label, &input.description, "session")?;
        let session = Session {
            id: a.id.clone(),
            label: input.label.trim().into(),
            description: input.description.trim().into(),
            elapsed_ms: a.elapsed_ms,
            started_at: a.started_at.clone(),
            finished_at: a.finished_at.clone().unwrap(),
            date: a.date.clone().unwrap(),
        };
        validate_session(&session)?;
        let tx = self.db.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO sessions VALUES (?1,?2,?3)",
            params![
                session.id,
                session.date,
                serde_json::to_string(&session).map_err(err)?
            ],
        )
        .map_err(err)?;
        tx.execute("DELETE FROM active_session", []).map_err(err)?;
        tx.commit().map_err(err)?;
        self.active_session = None;
        Ok(session)
    }
    pub fn discard_session(&mut self) -> Result<()> {
        if self.active.is_some() {
            return Err("Resolve the active stint before discarding the session.".into());
        }
        let id = self
            .active_session
            .as_ref()
            .ok_or("No active session.")?
            .id
            .clone();
        let tx = self.db.transaction().map_err(err)?;
        unlink_stints(&tx, &id)?;
        tx.execute("DELETE FROM active_session", []).map_err(err)?;
        tx.commit().map_err(err)?;
        self.active_session = None;
        Ok(())
    }

    pub fn start(
        &mut self,
        mode: Mode,
        label: String,
        minutes: Option<u64>,
        now: &Stamp,
    ) -> Result<()> {
        if self.active.is_some() {
            return Err("Finish or discard the current stint first.".into());
        }
        if mode == Mode::Manual {
            return Err("Manual stints are added from History.".into());
        }
        if label.len() > 240 {
            return Err("Keep labels under 240 bytes.".into());
        }
        if self
            .active_session
            .as_ref()
            .is_some_and(|s| s.phase != Phase::Running)
        {
            return Err("Resume the session before starting a stint.".into());
        }
        let target_ms = if mode == Mode::Countdown {
            let m = minutes.ok_or("Choose a countdown duration.")?;
            if m == 0 || m > MAX_DURATION / 60_000 {
                return Err("Choose a positive whole-minute duration (up to ten years).".into());
            }
            Some(m * 60_000)
        } else {
            None
        };
        self.set_active(Some(Active {
            id: Uuid::new_v4().to_string(),
            label: label.trim().into(),
            mode,
            target_ms,
            elapsed_ms: 0,
            started_at: now.iso(),
            finished_at: None,
            date: None,
            phase: Phase::Running,
            session_id: self.active_session.as_ref().map(|s| s.id.clone()),
            anchor_ms: now.mono_ms,
        }))
    }
    pub fn tick(&mut self, now: &Stamp) -> Result<bool> {
        if let Some(mut a) = self.snapshot(now) {
            if a.phase == Phase::Running
                && (a.target_ms == Some(a.elapsed_ms) || a.elapsed_ms >= MAX_DURATION)
            {
                let original = self.active.as_ref().unwrap();
                let due = original
                    .anchor_ms
                    .saturating_add(a.elapsed_ms.saturating_sub(original.elapsed_ms));
                let overshoot = now.mono_ms.saturating_sub(due).min(i64::MAX as u64);
                let finished = now.wall - chrono::Duration::milliseconds(overshoot as i64);
                a.phase = Phase::Review;
                a.finished_at = Some(finished.to_rfc3339());
                a.date = Some(finished.date_naive().to_string());
                self.set_active(Some(a))?;
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub fn checkpoint(&self, now: &Stamp) -> Result<()> {
        persist_singleton(&self.db, "active", &self.snapshot(now))?;
        persist_singleton(&self.db, "active_session", &self.session_snapshot(now))
    }
    pub fn pause(&mut self, now: &Stamp) -> Result<()> {
        self.tick(now)?;
        let mut a = self.snapshot(now).ok_or("No active stint.")?;
        if a.phase == Phase::Running {
            a.phase = Phase::Paused;
            self.set_active(Some(a))?;
        }
        Ok(())
    }
    pub fn resume(&mut self, now: &Stamp) -> Result<()> {
        let mut a = self.active.clone().ok_or("No active stint.")?;
        if a.phase == Phase::Review {
            return Err("Save or discard the ended stint.".into());
        }
        if self
            .active_session
            .as_ref()
            .is_some_and(|s| s.phase != Phase::Running)
        {
            return Err("Resume the session before resuming its stint.".into());
        }
        if a.phase == Phase::Paused {
            a.phase = Phase::Running;
            a.anchor_ms = now.mono_ms;
            self.set_active(Some(a))?;
        }
        Ok(())
    }
    pub fn finish(&mut self, now: &Stamp) -> Result<()> {
        self.tick(now)?;
        let mut a = self.snapshot(now).ok_or("No active stint.")?;
        if a.phase != Phase::Review {
            a.phase = Phase::Review;
            a.finished_at = Some(now.iso());
            a.date = Some(now.wall.date_naive().to_string());
            self.set_active(Some(a))?;
        }
        Ok(())
    }
    pub fn discard(&mut self) -> Result<()> {
        self.set_active(None)
    }
    pub fn save(&mut self, label: String, description: String) -> Result<Stint> {
        let a = self.active.as_ref().ok_or("No stint to save.")?;
        if a.phase != Phase::Review {
            return Err("End the stint before saving it.".into());
        }
        let stint = Stint {
            id: a.id.clone(),
            label: label.trim().into(),
            description: description.trim().into(),
            mode: a.mode,
            target_ms: a.target_ms,
            elapsed_ms: a.elapsed_ms,
            started_at: a.started_at.clone(),
            finished_at: a.finished_at.clone().unwrap(),
            date: a.date.clone().unwrap(),
            session_id: a.session_id.clone(),
        };
        validate_stint(&stint)?;
        self.validate_session_reference(stint.session_id.as_deref())?;
        let tx = self.db.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO stints VALUES (?1,?2,?3)",
            params![
                stint.id,
                stint.date,
                serde_json::to_string(&stint).map_err(err)?
            ],
        )
        .map_err(err)?;
        tx.execute("DELETE FROM active", []).map_err(err)?;
        tx.commit().map_err(err)?;
        self.active = None;
        Ok(stint)
    }

    pub fn list(&self) -> Result<Vec<Stint>> {
        read_records(&self.db, "stints")
    }
    pub fn list_sessions(&self) -> Result<Vec<Session>> {
        read_records(&self.db, "sessions")
    }
    pub fn summaries(&self) -> Result<BTreeMap<String, DaySummary>> {
        let sessions = self.list_sessions()?;
        let dates: BTreeMap<_, _> = sessions
            .iter()
            .map(|s| (s.id.clone(), s.date.clone()))
            .collect();
        let mut days: BTreeMap<String, DaySummary> = BTreeMap::new();
        for s in sessions {
            days.entry(s.date).or_default().session_count += 1;
        }
        for s in self.list()? {
            let date = s
                .session_id
                .as_ref()
                .and_then(|id| dates.get(id))
                .cloned()
                .unwrap_or(s.date);
            let day = days.entry(date).or_default();
            day.count += 1;
            day.elapsed_ms += s.elapsed_ms;
        }
        Ok(days)
    }
    pub fn upsert(&mut self, id: Option<String>, input: EntryInput, now: &Stamp) -> Result<Stint> {
        self.validate_session_reference(input.session_id.as_deref())?;
        let mut s = if let Some(id) = id {
            self.list()?
                .into_iter()
                .find(|s| s.id == id)
                .ok_or("Stint not found.")?
        } else {
            Stint {
                id: Uuid::new_v4().to_string(),
                label: String::new(),
                description: String::new(),
                mode: Mode::Manual,
                target_ms: None,
                elapsed_ms: 0,
                started_at: now.iso(),
                finished_at: now.iso(),
                date: String::new(),
                session_id: None,
            }
        };
        s.label = input.label.trim().into();
        s.description = input.description.trim().into();
        s.date = input.date;
        s.elapsed_ms = input.elapsed_ms;
        s.session_id = input.session_id;
        validate_stint(&s)?;
        self.db.execute("INSERT INTO stints VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET date=excluded.date,data=excluded.data", params![s.id,s.date,serde_json::to_string(&s).map_err(err)?]).map_err(err)?;
        Ok(s)
    }
    pub fn update_session(&mut self, id: &str, input: SessionInput) -> Result<Session> {
        validate_text(&input.label, &input.description, "session")?;
        let mut s = self
            .list_sessions()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or("Session not found.")?;
        s.label = input.label.trim().into();
        s.description = input.description.trim().into();
        validate_session(&s)?;
        self.db
            .execute(
                "UPDATE sessions SET data=?2 WHERE id=?1",
                params![s.id, serde_json::to_string(&s).map_err(err)?],
            )
            .map_err(err)?;
        Ok(s)
    }
    pub fn delete(&mut self, id: &str) -> Result<()> {
        if self
            .db
            .execute("DELETE FROM stints WHERE id=?1", [id])
            .map_err(err)?
            == 0
        {
            return Err("Stint not found.".into());
        }
        Ok(())
    }
    pub fn delete_session(&mut self, id: &str) -> Result<()> {
        let tx = self.db.transaction().map_err(err)?;
        unlink_stints(&tx, id)?;
        if tx
            .execute("DELETE FROM sessions WHERE id=?1", [id])
            .map_err(err)?
            == 0
        {
            return Err("Session not found.".into());
        }
        tx.commit().map_err(err)
    }
    fn validate_session_reference(&self, id: Option<&str>) -> Result<()> {
        let Some(id) = id else { return Ok(()) };
        if self.active_session.as_ref().is_some_and(|s| s.id == id)
            || self.list_sessions()?.iter().any(|s| s.id == id)
        {
            Ok(())
        } else {
            Err("Selected session was not found.".into())
        }
    }

    pub fn backup(&self) -> Result<String> {
        serde_json::to_string_pretty(&Backup {
            version: 2,
            sessions: self.list_sessions()?,
            stints: self.list()?,
        })
        .map_err(err)
    }
    fn parse_import(&self, json: &str) -> Result<(Backup, ImportPreview)> {
        let b: Backup = serde_json::from_str(json).map_err(|e| format!("Invalid backup: {e}"))?;
        if b.version != 1 && b.version != 2 {
            return Err("Unsupported backup version.".into());
        }
        if b.version == 1 && !b.sessions.is_empty() {
            return Err("Version 1 backups cannot contain sessions.".into());
        }
        let existing_stints: BTreeMap<_, _> = self
            .list()?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        let existing_sessions: BTreeMap<_, _> = self
            .list_sessions()?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();
        let imported_sessions: HashSet<_> = b.sessions.iter().map(|s| s.id.as_str()).collect();
        let mut seen = HashSet::new();
        let mut skipped = 0;
        for s in &b.sessions {
            validate_session(s)?;
            if !seen.insert(format!("session:{}", s.id)) {
                return Err("Duplicate session ID in backup.".into());
            }
            if self.active_session.as_ref().is_some_and(|a| a.id == s.id) {
                return Err("Backup conflicts with the active session.".into());
            }
            if let Some(old) = existing_sessions.get(&s.id) {
                if old != s {
                    return Err(format!(
                        "Conflicting session: {} ({}). No records imported.",
                        s.label, s.id
                    ));
                }
                skipped += 1;
            }
        }
        for s in &b.stints {
            validate_stint(s)?;
            if !seen.insert(format!("stint:{}", s.id)) {
                return Err("Duplicate stint ID in backup.".into());
            }
            if let Some(id) = s.session_id.as_deref() {
                if !imported_sessions.contains(id) && !existing_sessions.contains_key(id) {
                    return Err(format!(
                        "Stint {} references a session that is not present.",
                        s.id
                    ));
                }
            }
            if self.active.as_ref().is_some_and(|a| a.id == s.id) {
                return Err("Backup conflicts with the active stint.".into());
            }
            if let Some(old) = existing_stints.get(&s.id) {
                if old != s {
                    return Err(format!(
                        "Conflicting stint: {} ({}). No records imported.",
                        s.label, s.id
                    ));
                }
                skipped += 1;
            }
        }
        let total = b.sessions.len() + b.stints.len();
        Ok((
            b,
            ImportPreview {
                total,
                added: total - skipped,
                skipped,
            },
        ))
    }
    pub fn preview(&self, json: &str) -> Result<ImportPreview> {
        Ok(self.parse_import(json)?.1)
    }
    pub fn import(&mut self, json: &str) -> Result<ImportPreview> {
        let (b, p) = self.parse_import(json)?;
        let tx = self.db.transaction().map_err(err)?;
        for s in b.sessions {
            tx.execute(
                "INSERT OR IGNORE INTO sessions VALUES (?1,?2,?3)",
                params![s.id, s.date, serde_json::to_string(&s).map_err(err)?],
            )
            .map_err(err)?;
        }
        for s in b.stints {
            tx.execute(
                "INSERT OR IGNORE INTO stints VALUES (?1,?2,?3)",
                params![s.id, s.date, serde_json::to_string(&s).map_err(err)?],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(p)
    }
    pub fn csv(&self) -> Result<String> {
        let mut w = csv::Writer::from_writer(Vec::new());
        w.write_record([
            "record_type",
            "id",
            "session_id",
            "label",
            "description",
            "mode",
            "target_ms",
            "elapsed_ms",
            "started_at",
            "finished_at",
            "original_date",
            "effective_date",
        ])
        .map_err(err)?;
        let sessions = self.list_sessions()?;
        let dates: BTreeMap<_, _> = sessions
            .iter()
            .map(|s| (s.id.clone(), s.date.clone()))
            .collect();
        for s in sessions {
            w.write_record([
                "session".into(),
                s.id,
                String::new(),
                safe(s.label),
                safe(s.description),
                String::new(),
                String::new(),
                s.elapsed_ms.to_string(),
                s.started_at,
                s.finished_at,
                s.date.clone(),
                s.date,
            ])
            .map_err(err)?;
        }
        for s in self.list()? {
            let mode = match s.mode {
                Mode::CountUp => "count_up",
                Mode::Countdown => "countdown",
                Mode::Manual => "manual",
            };
            let effective = s
                .session_id
                .as_ref()
                .and_then(|id| dates.get(id))
                .cloned()
                .unwrap_or_else(|| s.date.clone());
            w.write_record([
                "stint".into(),
                s.id,
                s.session_id.unwrap_or_default(),
                safe(s.label),
                safe(s.description),
                mode.into(),
                s.target_ms.map(|v| v.to_string()).unwrap_or_default(),
                s.elapsed_ms.to_string(),
                s.started_at,
                s.finished_at,
                s.date,
                effective,
            ])
            .map_err(err)?;
        }
        String::from_utf8(w.into_inner().map_err(err)?).map_err(err)
    }
}

fn read_singleton<T: for<'de> Deserialize<'de>>(db: &Connection, table: &str) -> Result<Option<T>> {
    let sql = format!("SELECT data FROM {table} WHERE singleton=1");
    let json: Option<String> = db
        .query_row(&sql, [], |r| r.get(0))
        .optional()
        .map_err(err)?;
    json.map(|j| serde_json::from_str(&j).map_err(err))
        .transpose()
}
fn persist_singleton<T: Serialize>(db: &Connection, table: &str, value: &Option<T>) -> Result<()> {
    if let Some(v) = value {
        let sql=format!("INSERT INTO {table} VALUES (1,?1) ON CONFLICT(singleton) DO UPDATE SET data=excluded.data");
        db.execute(&sql, [serde_json::to_string(v).map_err(err)?])
            .map_err(err)?;
    } else {
        db.execute(&format!("DELETE FROM {table}"), [])
            .map_err(err)?;
    }
    Ok(())
}
fn read_records<T: for<'de> Deserialize<'de>>(db: &Connection, table: &str) -> Result<Vec<T>> {
    let mut q = db
        .prepare(&format!("SELECT data FROM {table} ORDER BY date DESC,id"))
        .map_err(err)?;
    let rows = q.query_map([], |r| r.get::<_, String>(0)).map_err(err)?;
    rows.map(|r| serde_json::from_str(&r.map_err(err)?).map_err(err))
        .collect()
}
fn unlink_stints(tx: &rusqlite::Transaction<'_>, session_id: &str) -> Result<()> {
    let mut q = tx.prepare("SELECT data FROM stints").map_err(err)?;
    let rows: Vec<String> = q
        .query_map([], |r| r.get(0))
        .map_err(err)?
        .collect::<std::result::Result<_, _>>()
        .map_err(err)?;
    drop(q);
    for json in rows {
        let mut s: Stint = serde_json::from_str(&json).map_err(err)?;
        if s.session_id.as_deref() == Some(session_id) {
            s.session_id = None;
            tx.execute(
                "UPDATE stints SET data=?2 WHERE id=?1",
                params![s.id, serde_json::to_string(&s).map_err(err)?],
            )
            .map_err(err)?;
        }
    }
    Ok(())
}
fn validate_text(label: &str, description: &str, kind: &str) -> Result<()> {
    if label.trim().is_empty() || label.len() > 240 {
        return Err(format!("A {kind} name is required (up to 240 bytes)."));
    }
    if description.len() > 20_000 {
        return Err("Notes are too long (maximum 20,000 bytes).".into());
    }
    Ok(())
}
fn validate_date(date: &str) -> Result<()> {
    let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "Invalid calendar date.")?;
    if d.to_string() != date {
        return Err("Use an ISO calendar date (YYYY-MM-DD).".into());
    }
    Ok(())
}
fn validate_stint(s: &Stint) -> Result<()> {
    Uuid::parse_str(&s.id).map_err(|_| "Invalid stint ID.")?;
    validate_text(&s.label, &s.description, "stint")?;
    if s.elapsed_ms == 0 || s.elapsed_ms > MAX_DURATION {
        return Err("Duration must be positive and no longer than ten years.".into());
    }
    validate_date(&s.date)?;
    DateTime::parse_from_rfc3339(&s.started_at).map_err(|_| "Invalid start timestamp.")?;
    DateTime::parse_from_rfc3339(&s.finished_at).map_err(|_| "Invalid finish timestamp.")?;
    match (s.mode, s.target_ms) {
        (Mode::Countdown, Some(v)) if v > 0 && v <= MAX_DURATION && v % 60_000 == 0 => {}
        (Mode::Countdown, _) => {
            return Err("Countdown needs a positive whole-minute target.".into())
        }
        (_, None) => {}
        (_, Some(_)) => return Err("Only countdowns have a target.".into()),
    }
    if let Some(id) = &s.session_id {
        Uuid::parse_str(id).map_err(|_| "Invalid session ID on stint.")?;
    }
    Ok(())
}
fn validate_session(s: &Session) -> Result<()> {
    Uuid::parse_str(&s.id).map_err(|_| "Invalid session ID.")?;
    validate_text(&s.label, &s.description, "session")?;
    if s.elapsed_ms > MAX_DURATION {
        return Err("Session duration cannot exceed ten years.".into());
    }
    validate_date(&s.date)?;
    DateTime::parse_from_rfc3339(&s.started_at).map_err(|_| "Invalid session start timestamp.")?;
    DateTime::parse_from_rfc3339(&s.finished_at)
        .map_err(|_| "Invalid session finish timestamp.")?;
    Ok(())
}
fn safe(value: String) -> String {
    if value
        .trim_start()
        .starts_with(['=', '+', '-', '@', '\t', '\r'])
    {
        format!("'{value}")
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stamp(ms: u64) -> Stamp {
        Stamp {
            mono_ms: ms,
            wall: DateTime::parse_from_rfc3339("2026-10-15T23:59:30+02:00").unwrap()
                + chrono::Duration::milliseconds(ms as i64),
        }
    }
    fn entry() -> EntryInput {
        EntryInput {
            label: "Read".into(),
            description: "Notes".into(),
            date: "2026-10-15".into(),
            elapsed_ms: 60_000,
            session_id: None,
        }
    }
    fn session_input() -> SessionInput {
        SessionInput {
            label: "Morning work".into(),
            description: "Plan".into(),
        }
    }
    #[test]
    fn standalone_timer_pause_and_save() {
        let mut e = Engine::memory().unwrap();
        e.start(Mode::CountUp, "Code".into(), None, &stamp(0))
            .unwrap();
        e.pause(&stamp(1_000)).unwrap();
        e.resume(&stamp(10_000)).unwrap();
        e.finish(&stamp(12_000)).unwrap();
        let s = e.save("Code".into(), "".into()).unwrap();
        assert_eq!(s.elapsed_ms, 3_000);
        assert_eq!(s.session_id, None);
    }
    #[test]
    fn session_includes_breaks_and_stints_join_it() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.start(Mode::CountUp, "One".into(), None, &stamp(1_000))
            .unwrap();
        e.finish(&stamp(6_000)).unwrap();
        let first = e.save("One".into(), "".into()).unwrap();
        e.start(Mode::CountUp, "Two".into(), None, &stamp(10_000))
            .unwrap();
        e.finish(&stamp(14_000)).unwrap();
        e.save("Two".into(), "".into()).unwrap();
        e.end_session(&stamp(20_000)).unwrap();
        let s = e.save_session(session_input()).unwrap();
        assert_eq!(s.elapsed_ms, 20_000);
        assert_eq!(first.session_id.as_deref(), Some(s.id.as_str()));
        assert_eq!(
            e.list().unwrap().iter().map(|s| s.elapsed_ms).sum::<u64>(),
            9_000
        );
    }
    #[test]
    fn session_pause_excludes_downtime_and_pauses_stint() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.start(Mode::CountUp, "One".into(), None, &stamp(1_000))
            .unwrap();
        e.pause_session(&stamp(5_000)).unwrap();
        assert_eq!(e.active.as_ref().unwrap().phase, Phase::Paused);
        assert_eq!(
            e.session_snapshot(&stamp(90_000)).unwrap().elapsed_ms,
            5_000
        );
        assert!(e.resume(&stamp(90_000)).is_err());
        e.resume_session(&stamp(90_000)).unwrap();
        e.resume(&stamp(91_000)).unwrap();
        e.finish(&stamp(92_000)).unwrap();
        assert_eq!(e.active.as_ref().unwrap().elapsed_ms, 5_000);
    }
    #[test]
    fn session_end_requires_resolved_stint() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.start(Mode::CountUp, "One".into(), None, &stamp(1_000))
            .unwrap();
        assert!(e.end_session(&stamp(2_000)).is_err());
        e.finish(&stamp(3_000)).unwrap();
        assert!(e.end_session(&stamp(4_000)).is_err());
        e.discard().unwrap();
        e.end_session(&stamp(5_000)).unwrap();
        assert_eq!(e.active_session.as_ref().unwrap().phase, Phase::Review);
    }
    #[test]
    fn countdown_expires_exactly_once() {
        let mut e = Engine::memory().unwrap();
        e.start(Mode::Countdown, "Timer".into(), Some(1), &stamp(0))
            .unwrap();
        assert!(e.tick(&stamp(65_000)).unwrap());
        assert!(!e.tick(&stamp(70_000)).unwrap());
        assert_eq!(e.active.as_ref().unwrap().elapsed_ms, 60_000);
    }
    #[test]
    fn recovery_pauses_both_clocks() {
        let path = std::env::temp_dir().join(format!("cadence-{}.sqlite", Uuid::new_v4()));
        {
            let mut e = Engine::open(&path).unwrap();
            e.start_session(session_input(), &stamp(0)).unwrap();
            e.start(Mode::CountUp, "One".into(), None, &stamp(1_000))
                .unwrap();
            e.checkpoint(&stamp(5_000)).unwrap();
        }
        {
            let e = Engine::open(&path).unwrap();
            assert_eq!(e.active.as_ref().unwrap().phase, Phase::Paused);
            assert_eq!(e.active_session.as_ref().unwrap().phase, Phase::Paused);
            assert_eq!(e.snapshot(&stamp(999_999)).unwrap().elapsed_ms, 4_000);
            assert_eq!(
                e.session_snapshot(&stamp(999_999)).unwrap().elapsed_ms,
                5_000
            );
        }
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn session_end_date_controls_history_and_delete_unlinks() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.start(Mode::CountUp, "One".into(), None, &stamp(1_000))
            .unwrap();
        e.finish(&stamp(2_000)).unwrap();
        let stint = e.save("One".into(), "".into()).unwrap();
        e.end_session(&stamp(40_000)).unwrap();
        let session = e.save_session(session_input()).unwrap();
        assert_eq!(session.date, "2026-10-16");
        assert_eq!(e.summaries().unwrap()["2026-10-16"].count, 1);
        e.delete_session(&session.id).unwrap();
        let unlinked = e.list().unwrap().pop().unwrap();
        assert_eq!(unlinked.id, stint.id);
        assert_eq!(unlinked.session_id, None);
        assert_eq!(e.summaries().unwrap()["2026-10-15"].count, 1);
    }
    #[test]
    fn zero_stint_session_is_valid() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.end_session(&stamp(1_000)).unwrap();
        e.save_session(session_input()).unwrap();
        let summaries = e.summaries().unwrap();
        let s = &summaries["2026-10-15"];
        assert_eq!(s.session_count, 1);
        assert_eq!(s.count, 0);
    }
    #[test]
    fn manual_entry_can_move_between_session_and_standalone() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.end_session(&stamp(1_000)).unwrap();
        let session = e.save_session(session_input()).unwrap();
        let mut input = entry();
        input.session_id = Some(session.id.clone());
        let stint = e.upsert(None, input.clone(), &stamp(0)).unwrap();
        input.session_id = None;
        e.upsert(Some(stint.id), input, &stamp(0)).unwrap();
        assert_eq!(e.list().unwrap()[0].session_id, None);
    }
    #[test]
    fn v1_database_migrates_and_v1_backup_imports() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE stints (id TEXT PRIMARY KEY,date TEXT NOT NULL,data TEXT NOT NULL);CREATE INDEX stints_date ON stints(date);CREATE TABLE active (singleton INTEGER PRIMARY KEY CHECK(singleton=1),data TEXT NOT NULL);PRAGMA user_version=1;").unwrap();
        let e = Engine::from_connection(db).unwrap();
        assert!(e.list_sessions().unwrap().is_empty());
        assert_eq!(e.preview("{\"version\":1,\"stints\":[]}").unwrap().total, 0);
    }
    #[test]
    fn v2_backup_roundtrip_validates_references() {
        let mut source = Engine::memory().unwrap();
        source.start_session(session_input(), &stamp(0)).unwrap();
        source
            .start(Mode::CountUp, "One".into(), None, &stamp(0))
            .unwrap();
        source.finish(&stamp(1_000)).unwrap();
        source.save("One".into(), "".into()).unwrap();
        source.end_session(&stamp(2_000)).unwrap();
        source.save_session(session_input()).unwrap();
        let json = source.backup().unwrap();
        let mut target = Engine::memory().unwrap();
        assert_eq!(target.import(&json).unwrap().added, 2);
        assert_eq!(target.import(&json).unwrap().skipped, 2);
        let mut invalid: Backup = serde_json::from_str(&json).unwrap();
        invalid.stints[0].session_id = Some(Uuid::new_v4().to_string());
        assert!(target
            .preview(&serde_json::to_string(&invalid).unwrap())
            .is_err());
    }
    #[test]
    fn csv_contains_session_and_stint_rows() {
        let mut e = Engine::memory().unwrap();
        e.start_session(session_input(), &stamp(0)).unwrap();
        e.end_session(&stamp(1_000)).unwrap();
        e.save_session(session_input()).unwrap();
        e.upsert(None, entry(), &stamp(0)).unwrap();
        let csv = e.csv().unwrap();
        assert!(csv.contains("record_type"));
        assert!(csv.contains("session,"));
        assert!(csv.contains("stint,"));
    }
}
