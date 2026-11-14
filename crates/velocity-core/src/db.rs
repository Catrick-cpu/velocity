//! Local SQLite persistence for activities, samples, routes and settings.
//!
//! Design goals: zero external services, WAL mode for crash safety, batched
//! sample inserts, and a schema that keeps long rides cheap to query.

use crate::route::Route;
use crate::types::{
    Activity, ActivitySummary, DataOrigin, Sample, Sport, now_ms,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

const SCHEMA_VERSION: i64 = 1;

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        let mut store = Self { conn, path };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut store = Self {
            conn,
            path: PathBuf::from(":memory:"),
        };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn configure(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA temp_store = MEMORY;",
        )?;
        Ok(())
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version >= SCHEMA_VERSION {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        if version < 1 {
            tx.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS activities (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    sport TEXT NOT NULL DEFAULT 'cycling',
                    started_at_ms INTEGER NOT NULL,
                    ended_at_ms INTEGER,
                    duration_s REAL NOT NULL DEFAULT 0,
                    moving_s REAL NOT NULL DEFAULT 0,
                    distance_m REAL NOT NULL DEFAULT 0,
                    elevation_gain_m REAL NOT NULL DEFAULT 0,
                    elevation_loss_m REAL NOT NULL DEFAULT 0,
                    avg_speed_mps REAL,
                    max_speed_mps REAL,
                    avg_power_w REAL,
                    max_power_w REAL,
                    normalized_power_w REAL,
                    avg_hr_bpm REAL,
                    max_hr_bpm REAL,
                    avg_cadence_rpm REAL,
                    calories_kcal REAL,
                    work_kj REAL,
                    tss REAL,
                    intensity_factor REAL,
                    route_id INTEGER,
                    route_name TEXT,
                    source TEXT NOT NULL DEFAULT '',
                    notes TEXT NOT NULL DEFAULT '',
                    created_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_activities_started
                    ON activities(started_at_ms DESC);

                CREATE TABLE IF NOT EXISTS samples (
                    activity_id INTEGER NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
                    t_ms INTEGER NOT NULL,
                    elapsed_s REAL NOT NULL,
                    lat REAL,
                    lon REAL,
                    altitude_m REAL,
                    distance_m REAL NOT NULL DEFAULT 0,
                    speed_mps REAL,
                    power_w REAL,
                    cadence_rpm REAL,
                    heart_rate_bpm REAL,
                    grade_pct REAL,
                    sim_flags INTEGER NOT NULL DEFAULT 0,
                    origin TEXT NOT NULL DEFAULT 'unknown',
                    PRIMARY KEY(activity_id, t_ms)
                ) WITHOUT ROWID;
                CREATE INDEX IF NOT EXISTS idx_samples_elapsed
                    ON samples(activity_id, elapsed_s);

                CREATE TABLE IF NOT EXISTS routes (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    distance_m REAL NOT NULL DEFAULT 0,
                    elevation_gain_m REAL NOT NULL DEFAULT 0,
                    is_demo INTEGER NOT NULL DEFAULT 0,
                    data TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS app_state (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                "#,
            )?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn create_activity(
        &mut self,
        name: &str,
        sport: Sport,
        started_at_ms: i64,
        source: &str,
        route_id: Option<i64>,
        route_name: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO activities
             (name, sport, started_at_ms, ended_at_ms, source, route_id, route_name, created_at_ms)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7)",
            params![
                name,
                sport.key(),
                started_at_ms,
                source,
                route_id,
                route_name,
                now_ms()
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert samples idempotently (duplicates on timestamp are replaced).
    pub fn append_samples(&mut self, activity_id: i64, samples: &[Sample]) -> Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO samples
                 (activity_id, t_ms, elapsed_s, lat, lon, altitude_m, distance_m, speed_mps,
                  power_w, cadence_rpm, heart_rate_bpm, grade_pct, sim_flags, origin)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            )?;
            for s in samples {
                stmt.execute(params![
                    activity_id,
                    s.t_ms,
                    s.elapsed_s,
                    s.lat,
                    s.lon,
                    s.altitude_m,
                    s.distance_m,
                    s.speed_mps,
                    s.power_w,
                    s.cadence_rpm,
                    s.heart_rate_bpm,
                    s.grade_pct,
                    s.sim_flags as i64,
                    origin_key(s.origin),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn finalize_activity(
        &mut self,
        id: i64,
        ended_at_ms: i64,
        summary: &ActivitySummary,
        name: &str,
        notes: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE activities SET
                name = ?2,
                ended_at_ms = ?3,
                duration_s = ?4,
                moving_s = ?5,
                distance_m = ?6,
                elevation_gain_m = ?7,
                elevation_loss_m = ?8,
                avg_speed_mps = ?9,
                max_speed_mps = ?10,
                avg_power_w = ?11,
                max_power_w = ?12,
                normalized_power_w = ?13,
                avg_hr_bpm = ?14,
                max_hr_bpm = ?15,
                avg_cadence_rpm = ?16,
                calories_kcal = ?17,
                work_kj = ?18,
                tss = ?19,
                intensity_factor = ?20,
                notes = ?21
             WHERE id = ?1",
            params![
                id,
                name,
                ended_at_ms,
                summary.duration_s,
                summary.moving_s,
                summary.distance_m,
                summary.elevation_gain_m,
                summary.elevation_loss_m,
                summary.avg_speed_mps,
                summary.max_speed_mps,
                summary.avg_power_w,
                summary.max_power_w,
                summary.normalized_power_w,
                summary.avg_heart_rate_bpm,
                summary.max_heart_rate_bpm,
                summary.avg_cadence_rpm,
                summary.calories_kcal,
                summary.work_kj,
                summary.tss,
                summary.intensity_factor,
                notes,
            ],
        )?;
        Ok(())
    }

    pub fn list_activities(&self, limit: usize, offset: usize) -> Result<Vec<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, sport, started_at_ms, ended_at_ms, duration_s, moving_s,
                    distance_m, elevation_gain_m, elevation_loss_m, avg_speed_mps,
                    max_speed_mps, avg_power_w, max_power_w, normalized_power_w,
                    avg_hr_bpm, max_hr_bpm, avg_cadence_rpm, calories_kcal, work_kj,
                    tss, intensity_factor, route_id, route_name, source, notes, created_at_ms
             FROM activities
             WHERE ended_at_ms IS NOT NULL
             ORDER BY started_at_ms DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], row_to_activity)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn activity_count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM activities WHERE ended_at_ms IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn get_activity(&self, id: i64) -> Result<Option<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, sport, started_at_ms, ended_at_ms, duration_s, moving_s,
                    distance_m, elevation_gain_m, elevation_loss_m, avg_speed_mps,
                    max_speed_mps, avg_power_w, max_power_w, normalized_power_w,
                    avg_hr_bpm, max_hr_bpm, avg_cadence_rpm, calories_kcal, work_kj,
                    tss, intensity_factor, route_id, route_name, source, notes, created_at_ms
             FROM activities WHERE id = ?1",
        )?;
        let activity = stmt.query_row(params![id], row_to_activity).optional()?;
        Ok(activity)
    }

    pub fn load_samples(&self, activity_id: i64) -> Result<Vec<Sample>> {
        let mut stmt = self.conn.prepare(
            "SELECT t_ms, elapsed_s, lat, lon, altitude_m, distance_m, speed_mps, power_w,
                    cadence_rpm, heart_rate_bpm, grade_pct, sim_flags, origin
             FROM samples WHERE activity_id = ?1 ORDER BY elapsed_s ASC, t_ms ASC",
        )?;
        let rows = stmt.query_map(params![activity_id], |r| {
            Ok(Sample {
                t_ms: r.get(0)?,
                elapsed_s: r.get(1)?,
                lat: r.get(2)?,
                lon: r.get(3)?,
                altitude_m: r.get(4)?,
                distance_m: r.get(5)?,
                speed_mps: r.get(6)?,
                power_w: r.get(7)?,
                cadence_rpm: r.get(8)?,
                heart_rate_bpm: r.get(9)?,
                grade_pct: r.get(10)?,
                sim_flags: r.get::<_, i64>(11)? as u16,
                origin: origin_from_key(&r.get::<_, String>(12)?),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Most recent activity that has no end timestamp (crash recovery).
    pub fn unfinished_activity(&self) -> Result<Option<Activity>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, sport, started_at_ms, ended_at_ms, duration_s, moving_s,
                    distance_m, elevation_gain_m, elevation_loss_m, avg_speed_mps,
                    max_speed_mps, avg_power_w, max_power_w, normalized_power_w,
                    avg_hr_bpm, max_hr_bpm, avg_cadence_rpm, calories_kcal, work_kj,
                    tss, intensity_factor, route_id, route_name, source, notes, created_at_ms
             FROM activities WHERE ended_at_ms IS NULL
             ORDER BY started_at_ms DESC LIMIT 1",
        )?;
        let activity = stmt.query_row([], row_to_activity).optional()?;
        Ok(activity)
    }

    pub fn update_activity_meta(
        &mut self,
        id: i64,
        name: &str,
        notes: &str,
        sport: Sport,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE activities SET name = ?2, notes = ?3, sport = ?4 WHERE id = ?1",
            params![id, name, notes, sport.key()],
        )?;
        Ok(())
    }

    pub fn delete_activity(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM activities WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn save_route(&mut self, route: &Route) -> Result<i64> {
        let data = serde_json::to_string(route)?;
        self.conn.execute(
            "INSERT INTO routes (name, distance_m, elevation_gain_m, is_demo, data, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                route.name,
                route.stats.distance_m,
                route.stats.elevation_gain_m,
                route.is_demo as i64,
                data,
                now_ms()
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_route(&mut self, id: i64, route: &Route) -> Result<()> {
        let data = serde_json::to_string(route)?;
        self.conn.execute(
            "UPDATE routes SET name = ?2, distance_m = ?3, elevation_gain_m = ?4,
                is_demo = ?5, data = ?6 WHERE id = ?1",
            params![
                id,
                route.name,
                route.stats.distance_m,
                route.stats.elevation_gain_m,
                route.is_demo as i64,
                data
            ],
        )?;
        Ok(())
    }

    pub fn list_routes(&self) -> Result<Vec<(i64, Route)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, data FROM routes ORDER BY is_demo ASC, created_at_ms DESC")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, data) = row?;
            match serde_json::from_str::<Route>(&data) {
                Ok(route) => out.push((id, route)),
                Err(e) => log::warn!("skipping corrupt route {id}: {e}"),
            }
        }
        Ok(out)
    }

    pub fn get_route(&self, id: i64) -> Result<Option<Route>> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM routes WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(match data {
            Some(s) => Some(serde_json::from_str(&s)?),
            None => None,
        })
    }

    pub fn delete_route(&mut self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM routes WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_setting(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_app_state(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn set_app_state(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO app_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Consistent online backup using SQLite's VACUUM INTO.
    pub fn backup_to(&self, target: impl AsRef<Path>) -> Result<()> {
        let target = target.as_ref();
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.conn
            .execute("VACUUM INTO ?1", params![target.to_string_lossy()])?;
        Ok(())
    }

    pub fn database_size_bytes(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }

    /// Seed a set of realistic demo activities when the database is empty.
    pub fn seed_demo_if_empty(&mut self) -> Result<bool> {
        if self.activity_count()? > 0 {
            return Ok(false);
        }
        let seeds = crate::demo::demo_activities();
        for (activity, samples) in seeds {
            let id = self.create_activity(
                &activity.name,
                activity.sport,
                activity.started_at_ms,
                &activity.source,
                activity.route_id,
                activity.route_name.as_deref(),
            )?;
            self.append_samples(id, &samples)?;
            self.finalize_activity(
                id,
                activity.ended_at_ms.unwrap_or(activity.started_at_ms),
                &activity.summary,
                &activity.name,
                &activity.notes,
            )?;
        }
        Ok(true)
    }
}

fn row_to_activity(r: &rusqlite::Row<'_>) -> rusqlite::Result<Activity> {
    let summary = ActivitySummary {
        duration_s: r.get(5)?,
        moving_s: r.get(6)?,
        distance_m: r.get(7)?,
        elevation_gain_m: r.get(8)?,
        elevation_loss_m: r.get(9)?,
        avg_speed_mps: r.get(10)?,
        max_speed_mps: r.get(11)?,
        avg_power_w: r.get(12)?,
        max_power_w: r.get(13)?,
        normalized_power_w: r.get(14)?,
        avg_heart_rate_bpm: r.get(15)?,
        max_heart_rate_bpm: r.get(16)?,
        avg_cadence_rpm: r.get(17)?,
        calories_kcal: r.get(18)?,
        work_kj: r.get(19)?,
        tss: r.get(20)?,
        intensity_factor: r.get(21)?,
    };
    Ok(Activity {
        id: r.get(0)?,
        name: r.get(1)?,
        sport: Sport::from_key(&r.get::<_, String>(2)?),
        started_at_ms: r.get(3)?,
        ended_at_ms: r.get(4)?,
        summary,
        route_id: r.get(22)?,
        route_name: r.get(23)?,
        source: r.get(24)?,
        notes: r.get(25)?,
        created_at_ms: r.get(26)?,
    })
}

fn origin_key(origin: DataOrigin) -> &'static str {
    match origin {
        DataOrigin::Unknown => "unknown",
        DataOrigin::Bluetooth => "bluetooth",
        DataOrigin::AntPlus => "ant_plus",
        DataOrigin::FileImport => "file_import",
        DataOrigin::Simulated => "simulated",
        DataOrigin::Manual => "manual",
        DataOrigin::Gps => "gps",
    }
}

fn origin_from_key(key: &str) -> DataOrigin {
    match key {
        "bluetooth" => DataOrigin::Bluetooth,
        "ant_plus" => DataOrigin::AntPlus,
        "file_import" => DataOrigin::FileImport,
        "simulated" => DataOrigin::Simulated,
        "manual" => DataOrigin::Manual,
        "gps" => DataOrigin::Gps,
        _ => DataOrigin::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Reading, DataOrigin};

    fn sample(i: i64) -> Sample {
        Sample {
            t_ms: i * 1000,
            elapsed_s: i as f64,
            lat: Some(48.0),
            lon: Some(11.0),
            altitude_m: Some(500.0),
            distance_m: i as f64 * 8.0,
            speed_mps: Some(8.0),
            power_w: Some(200.0),
            cadence_rpm: Some(90.0),
            heart_rate_bpm: Some(150.0),
            grade_pct: Some(1.0),
            sim_flags: crate::types::sim::POWER,
            origin: DataOrigin::Bluetooth,
        }
    }

    #[test]
    fn roundtrip_activity_and_samples() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .create_activity("Test Ride", Sport::Cycling, 1_700_000_000_000, "test", None, None)
            .unwrap();
        let samples: Vec<Sample> = (0..10).map(sample).collect();
        store.append_samples(id, &samples).unwrap();
        let loaded = store.load_samples(id).unwrap();
        assert_eq!(loaded.len(), 10);
        assert_eq!(loaded[3].power_w, Some(200.0));
        assert_eq!(loaded[3].origin, DataOrigin::Bluetooth);

        let mut summary = ActivitySummary::default();
        summary.duration_s = 9.0;
        summary.distance_m = 72.0;
        summary.tss = Some(42.0);
        store
            .finalize_activity(id, 1_700_000_010_000, &summary, "Test Ride", "note")
            .unwrap();
        let a = store.get_activity(id).unwrap().unwrap();
        assert_eq!(a.summary.tss, Some(42.0));
        assert!(a.ended_at_ms.is_some());
        assert_eq!(store.activity_count().unwrap(), 1);
        assert!(store.unfinished_activity().unwrap().is_none());
    }

    #[test]
    fn duplicate_samples_are_replaced() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .create_activity("Dup", Sport::Cycling, 1, "test", None, None)
            .unwrap();
        store.append_samples(id, &[sample(0), sample(0)]).unwrap();
        assert_eq!(store.load_samples(id).unwrap().len(), 1);
    }

    #[test]
    fn unfinished_activity_is_detected() {
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .create_activity("Crashed", Sport::Cycling, 1, "test", None, None)
            .unwrap();
        store.append_samples(id, &[sample(0), sample(1)]).unwrap();
        let a = store.unfinished_activity().unwrap().unwrap();
        assert_eq!(a.id, id);
        assert_eq!(store.load_samples(id).unwrap().len(), 2);
    }

    #[test]
    fn routes_roundtrip() {
        let mut store = Store::open_in_memory().unwrap();
        let mut route = crate::route::demo_route();
        route.name = "Saved Loop".into();
        let id = store.save_route(&route).unwrap();
        let all = store.list_routes().unwrap();
        assert_eq!(all.len(), 1);
        let loaded = store.get_route(id).unwrap().unwrap();
        assert_eq!(loaded.name, "Saved Loop");
        assert!(loaded.points.len() > 500);
    }

    #[test]
    fn settings_persist() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(store.get_setting("theme").unwrap(), None);
        store.set_setting("theme", "dark").unwrap();
        assert_eq!(store.get_setting("theme").unwrap().as_deref(), Some("dark"));
        store.set_setting("theme", "light").unwrap();
        assert_eq!(store.get_setting("theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn backup_creates_file() {
        let dir = std::env::temp_dir().join(format!("velocity-test-{}", now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("data.db");
        let backup = dir.join("backup.db");
        let mut store = Store::open(&db_path).unwrap();
        store
            .create_activity("Backup", Sport::Cycling, 1, "test", None, None)
            .unwrap();
        store.backup_to(&backup).unwrap();
        assert!(backup.exists());
        assert!(backup.metadata().unwrap().len() > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unused_reading_helper_compiles() {
        let r = Reading::new(1.0, 1, false, DataOrigin::Bluetooth);
        assert_eq!(r.value, 1.0);
    }
}
