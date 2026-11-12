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
            genre           TEXT,
            subgenre        TEXT,
            duration_secs   REAL,
            bpm             REAL,
            bpm_confidence  REAL,
            key_camelot     TEXT,
            key_name        TEXT,
            key_confidence  REAL,
            energy          REAL,
            artwork_path    TEXT,
            waveform_peaks  TEXT,
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

        -- A transition belongs to exactly one gap between two setlist
        -- entries: its start/stop points depend on the two specific songs
        -- involved, so unlike a track or a setlist, there's nothing to
        -- usefully name and reuse elsewhere. It lives inline on the entry
        -- rather than as a separate shared/"saved" table.
        CREATE TABLE IF NOT EXISTS setlist_entries (
            id                TEXT PRIMARY KEY,
            setlist_id        TEXT NOT NULL REFERENCES setlists(id) ON DELETE CASCADE,
            track_id          TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            position          INTEGER NOT NULL,
            transition_type   TEXT,
            crossfade_secs    REAL,
            cue_out_secs      REAL,
            cue_in_secs       REAL,
            transition_notes  TEXT
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

    // Additive migrations for databases created before this column existed.
    // SQLite has no "ADD COLUMN IF NOT EXISTS"; ignore the duplicate-column
    // error on a database that already has it.
    conn.execute("ALTER TABLE tracks ADD COLUMN genre TEXT", []).ok();
    conn.execute("ALTER TABLE tracks ADD COLUMN subgenre TEXT", []).ok();
    // JSON-encoded Vec<(f32, f32)> of (min, max) amplitude per downsampled
    // bucket — computed once at analysis time so the UI can draw a real
    // waveform without holding full decoded sample buffers in memory.
    conn.execute("ALTER TABLE tracks ADD COLUMN waveform_peaks TEXT", []).ok();

    // Transitions moved from a separate shared/"saved" table onto the
    // setlist_entries row they actually belong to (see the comment above
    // that table) — add the new inline columns for databases created before
    // this change, and drop the now-unused table. Nothing worth preserving
    // in it: a saved transition's cue points were only ever meaningful for
    // the specific track pair they were created against anyway.
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN transition_type TEXT", []).ok();
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN crossfade_secs REAL", []).ok();
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN cue_out_secs REAL", []).ok();
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN cue_in_secs REAL", []).ok();
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN transition_notes TEXT", []).ok();
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN transition_enabled INTEGER NOT NULL DEFAULT 1", []).ok();

    // A database from before this change still has the old `transition_id`
    // column, which carries `REFERENCES transitions(id)` — a foreign key to
    // the table we're about to drop. Under `PRAGMA foreign_keys = ON`
    // (above), SQLite refuses to even *prepare* a write against a table
    // whose schema references a now-missing table ("no such table:
    // main.transitions") — not just inserts into the dangling column, any
    // write to setlist_entries at all, including plain deletes. Dropping
    // `transitions` without also removing this column breaks every "remove
    // track from setlist" click on such a database. SQLite can't drop a
    // column that's part of a foreign key directly, so rebuild the table
    // without it instead — the standard, version-independent way to do
    // this in SQLite.
    let has_old_fk_column: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('setlist_entries') WHERE name = 'transition_id'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    if has_old_fk_column {
        conn.execute_batch(
            r#"
            CREATE TABLE setlist_entries_new (
                id                TEXT PRIMARY KEY,
                setlist_id        TEXT NOT NULL REFERENCES setlists(id) ON DELETE CASCADE,
                track_id          TEXT NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
                position          INTEGER NOT NULL,
                transition_type   TEXT,
                crossfade_secs    REAL,
                cue_out_secs      REAL,
                cue_in_secs       REAL,
                transition_notes  TEXT
            );
            INSERT INTO setlist_entries_new
                (id, setlist_id, track_id, position, transition_type, crossfade_secs, cue_out_secs, cue_in_secs, transition_notes)
                SELECT id, setlist_id, track_id, position, transition_type, crossfade_secs, cue_out_secs, cue_in_secs, transition_notes
                FROM setlist_entries;
            DROP TABLE setlist_entries;
            ALTER TABLE setlist_entries_new RENAME TO setlist_entries;
            CREATE INDEX IF NOT EXISTS idx_setlist_entries_setlist ON setlist_entries(setlist_id);
            "#,
        )
        .expect("failed to rebuild setlist_entries without the stale transitions foreign key");
    }

    // (Re-)add after the rebuild above, which recreates the table without it.
    conn.execute("ALTER TABLE setlist_entries ADD COLUMN transition_enabled INTEGER NOT NULL DEFAULT 1", []).ok();

    conn.execute("DROP TABLE IF EXISTS transitions", []).ok();

    Db(Mutex::new(conn))
}
