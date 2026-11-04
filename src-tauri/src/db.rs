use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("Gigflow");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn init() -> Db {
    let path = app_data_dir().join("gigflow.db");
    let conn = Connection::open(path).expect("failed to open database");
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS tracks (
            id              TEXT PRIMARY KEY,
            file_path       TEXT NOT NULL UNIQUE,
            title           TEXT NOT NULL,
            artist          TEXT,
            album           TEXT,
            duration_secs   REAL,
            bpm             REAL,
            bpm_confidence  REAL,
            key_camelot     TEXT,
            key_name        TEXT,
            key_confidence  REAL,
            energy          REAL,
            artwork_path    TEXT,
            date_added      TEXT NOT NULL,
            date_analyzed   TEXT
        );

        CREATE TABLE IF NOT EXISTS setlists (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            gig_date    TEXT,
            notes       TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS setlist_entries (
            id              TEXT PRIMARY KEY,
            setlist_id      TEXT NOT NULL REFERENCES setlists(id) ON DELETE CASCADE,
            track_id        TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            position        INTEGER NOT NULL,
            transition_id   TEXT REFERENCES transitions(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS transitions (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            transition_type TEXT NOT NULL,
            crossfade_secs  REAL NOT NULL DEFAULT 8,
            cue_out_secs    REAL,
            cue_in_secs     REAL,
            notes           TEXT,
            from_key        TEXT,
            to_key          TEXT,
            visibility      TEXT NOT NULL DEFAULT 'private',
            author          TEXT,
            created_at      TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS hot_cues (
            id              TEXT PRIMARY KEY,
            track_id        TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            pad             INTEGER NOT NULL,
            position_secs   REAL NOT NULL,
            label           TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS profile (
            id              TEXT PRIMARY KEY,
            display_name    TEXT NOT NULL DEFAULT '',
            bio             TEXT NOT NULL DEFAULT '',
            socials_json    TEXT NOT NULL DEFAULT '[]',
            work_links_json TEXT NOT NULL DEFAULT '[]',
            avatar_path     TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_setlist_entries_setlist ON setlist_entries(setlist_id);
        CREATE INDEX IF NOT EXISTS idx_hot_cues_track ON hot_cues(track_id);
        "#,
    )
    .expect("failed to run migrations");

    Db(Mutex::new(conn))
}
