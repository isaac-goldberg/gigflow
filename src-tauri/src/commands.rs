use crate::audio;
use crate::audio::key::camelot_relation;
use crate::db::Db;
use crate::export;
use crate::models::*;
use chrono::Utc;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;
use rusqlite::{params, Row};
use std::path::{Path, PathBuf};
use tauri::State;
use uuid::Uuid;

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn track_from_row(row: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get("id")?,
        file_path: row.get("file_path")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        duration_secs: row.get("duration_secs")?,
        bpm: row.get("bpm")?,
        bpm_confidence: row.get("bpm_confidence")?,
        key_camelot: row.get("key_camelot")?,
        key_name: row.get("key_name")?,
        key_confidence: row.get("key_confidence")?,
        energy: row.get("energy")?,
        artwork_path: row.get("artwork_path")?,
        date_added: row.get("date_added")?,
        date_analyzed: row.get("date_analyzed")?,
    })
}

fn hot_cue_from_row(row: &Row) -> rusqlite::Result<HotCue> {
    Ok(HotCue {
        id: row.get("id")?,
        track_id: row.get("track_id")?,
        pad: row.get("pad")?,
        position_secs: row.get("position_secs")?,
        label: row.get("label")?,
    })
}

fn setlist_from_row(row: &Row) -> rusqlite::Result<Setlist> {
    Ok(Setlist {
        id: row.get("id")?,
        name: row.get("name")?,
        gig_date: row.get("gig_date")?,
        notes: row.get("notes")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn transition_from_row(row: &Row) -> rusqlite::Result<Transition> {
    Ok(Transition {
        id: row.get("id")?,
        name: row.get("name")?,
        transition_type: row.get("transition_type")?,
        crossfade_secs: row.get("crossfade_secs")?,
        cue_out_secs: row.get("cue_out_secs")?,
        cue_in_secs: row.get("cue_in_secs")?,
        notes: row.get("notes")?,
        from_key: row.get("from_key")?,
        to_key: row.get("to_key")?,
        visibility: row.get("visibility")?,
        author: row.get("author")?,
        created_at: row.get("created_at")?,
    })
}

// ---------- Library ----------

#[tauri::command]
pub fn import_tracks(db: State<Db>, paths: Vec<String>) -> Result<Vec<Track>, String> {
    let conn = db.0.lock().unwrap();
    let mut imported = Vec::new();

    for path_str in paths {
        let path = PathBuf::from(&path_str);
        let (title, artist, album, duration_secs) = read_tags(&path);
        let id = Uuid::new_v4().to_string();
        let date_added = now();

        let result = conn.execute(
            "INSERT INTO tracks (id, file_path, title, artist, album, duration_secs, date_added)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(file_path) DO NOTHING",
            params![id, path_str, title, artist, album, duration_secs, date_added],
        );
        if let Err(e) = result {
            return Err(format!("failed to import {path_str}: {e}"));
        }

        if let Ok(track) = conn.query_row(
            "SELECT * FROM tracks WHERE file_path = ?1",
            params![path_str],
            track_from_row,
        ) {
            imported.push(track);
        }
    }

    Ok(imported)
}

fn read_tags(path: &Path) -> (String, Option<String>, Option<String>, Option<f64>) {
    let fallback_title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Track".to_string());

    match lofty::read_from_path(path) {
        Ok(tagged) => {
            let duration = tagged.properties().duration().as_secs_f64();
            let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
            let title = tag
                .and_then(|t| t.title())
                .map(|c| c.to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback_title);
            let artist = tag.and_then(|t| t.artist()).map(|c| c.to_string());
            let album = tag.and_then(|t| t.album()).map(|c| c.to_string());
            (title, artist, album, Some(duration))
        }
        Err(_) => (fallback_title, None, None, None),
    }
}

#[tauri::command]
pub fn analyze_track(db: State<Db>, track_id: String) -> Result<Track, String> {
    let file_path: String = {
        let conn = db.0.lock().unwrap();
        conn.query_row(
            "SELECT file_path FROM tracks WHERE id = ?1",
            params![track_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("track not found: {e}"))?
    };

    let result = audio::analyze_file(Path::new(&file_path))?;

    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE tracks SET duration_secs = ?1, bpm = ?2, bpm_confidence = ?3,
         key_camelot = ?4, key_name = ?5, key_confidence = ?6, energy = ?7, date_analyzed = ?8
         WHERE id = ?9",
        params![
            result.duration_secs,
            result.bpm,
            result.bpm_confidence,
            result.key_camelot,
            result.key_name,
            result.key_confidence,
            result.energy,
            now(),
            track_id,
        ],
    )
    .map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM hot_cues WHERE track_id = ?1", params![track_id])
        .map_err(|e| e.to_string())?;
    for (i, cue) in result.hot_cues.iter().enumerate() {
        conn.execute(
            "INSERT INTO hot_cues (id, track_id, pad, position_secs, label) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), track_id, (i + 1) as i64, cue.position_secs, cue.label],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.query_row(
        "SELECT * FROM tracks WHERE id = ?1",
        params![track_id],
        track_from_row,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tracks(db: State<Db>) -> Result<Vec<Track>, String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT * FROM tracks ORDER BY date_added DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], track_from_row)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_track(
    db: State<Db>,
    track_id: String,
    bpm: Option<f64>,
    key_camelot: Option<String>,
    key_name: Option<String>,
    title: Option<String>,
    artist: Option<String>,
) -> Result<Track, String> {
    let conn = db.0.lock().unwrap();
    if let Some(v) = bpm {
        conn.execute("UPDATE tracks SET bpm = ?1, bpm_confidence = 1.0 WHERE id = ?2", params![v, track_id]).ok();
    }
    if let Some(v) = &key_camelot {
        conn.execute("UPDATE tracks SET key_camelot = ?1, key_confidence = 1.0 WHERE id = ?2", params![v, track_id]).ok();
    }
    if let Some(v) = &key_name {
        conn.execute("UPDATE tracks SET key_name = ?1 WHERE id = ?2", params![v, track_id]).ok();
    }
    if let Some(v) = &title {
        conn.execute("UPDATE tracks SET title = ?1 WHERE id = ?2", params![v, track_id]).ok();
    }
    if let Some(v) = &artist {
        conn.execute("UPDATE tracks SET artist = ?1 WHERE id = ?2", params![v, track_id]).ok();
    }
    conn.query_row("SELECT * FROM tracks WHERE id = ?1", params![track_id], track_from_row)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_track(db: State<Db>, track_id: String) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM tracks WHERE id = ?1", params![track_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Hot cues ----------

#[tauri::command]
pub fn get_hot_cues(db: State<Db>, track_id: String) -> Result<Vec<HotCue>, String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT * FROM hot_cues WHERE track_id = ?1 ORDER BY position_secs ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![track_id], hot_cue_from_row).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_hot_cue(db: State<Db>, cue_id: String) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM hot_cues WHERE id = ?1", params![cue_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Suggestions ----------

#[tauri::command]
pub fn suggest_next_tracks(
    db: State<Db>,
    current_track_id: String,
    exclude_ids: Vec<String>,
) -> Result<Vec<TrackSuggestion>, String> {
    let conn = db.0.lock().unwrap();
    let current = conn
        .query_row("SELECT * FROM tracks WHERE id = ?1", params![current_track_id], track_from_row)
        .map_err(|e| format!("current track not found: {e}"))?;

    let mut stmt = conn.prepare("SELECT * FROM tracks WHERE id != ?1").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![current_track_id], track_from_row)
        .map_err(|e| e.to_string())?;
    let candidates: Vec<Track> = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

    let mut suggestions: Vec<TrackSuggestion> = candidates
        .into_iter()
        .filter(|t| !exclude_ids.contains(&t.id))
        .map(|t| score_candidate(&current, t))
        .collect();

    suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    suggestions.truncate(8);
    Ok(suggestions)
}

fn score_candidate(current: &Track, candidate: Track) -> TrackSuggestion {
    let bpm_score = match (current.bpm, candidate.bpm) {
        (Some(a), Some(b)) if a > 0.0 => {
            let delta_pct = (a - b).abs() / a;
            (1.0 - delta_pct / 0.08).clamp(0.0, 1.0)
        }
        _ => 0.3,
    };

    let (key_score, key_relation) = match (&current.key_camelot, &candidate.key_camelot) {
        (Some(a), Some(b)) => {
            let rel = camelot_relation(a, b);
            let score = match rel {
                "same key" => 1.0,
                "relative major/minor" => 0.9,
                "adjacent (perfect mix)" => 0.85,
                "energy shift" => 0.5,
                "clash" => 0.15,
                _ => 0.3,
            };
            (score, rel.to_string())
        }
        _ => (0.3, "unknown".to_string()),
    };

    let bpm_delta = match (current.bpm, candidate.bpm) {
        (Some(a), Some(b)) => Some(b - a),
        _ => None,
    };

    // Energy: reward a gentle hold-or-build (DJs rarely want a sudden crash
    // mid-set), penalize both a big drop and an overly jarring spike.
    let (energy_score, energy_delta) = match (current.energy, candidate.energy) {
        (Some(a), Some(b)) => {
            let delta = b - a;
            let score = if delta < 0.0 {
                (1.0 + delta / 0.35).clamp(0.0, 1.0)
            } else {
                (1.0 - delta / 0.5).clamp(0.0, 1.0)
            };
            (score, Some(delta))
        }
        _ => (0.5, None),
    };

    TrackSuggestion {
        score: 0.4 * bpm_score + 0.4 * key_score + 0.2 * energy_score,
        bpm_delta,
        key_relation,
        energy_delta,
        track: candidate,
    }
}

// ---------- Setlists ----------

#[tauri::command]
pub fn create_setlist(db: State<Db>, name: String, gig_date: Option<String>, notes: Option<String>) -> Result<Setlist, String> {
    let conn = db.0.lock().unwrap();
    let id = Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO setlists (id, name, gig_date, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, name, gig_date, notes, ts],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row("SELECT * FROM setlists WHERE id = ?1", params![id], setlist_from_row)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_setlists(db: State<Db>) -> Result<Vec<Setlist>, String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare("SELECT * FROM setlists ORDER BY updated_at DESC").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], setlist_from_row).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_setlist(db: State<Db>, setlist_id: String) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM setlists WHERE id = ?1", params![setlist_id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_setlist_entries(db: State<Db>, setlist_id: String) -> Result<Vec<SetlistEntryWithTrack>, String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT * FROM setlist_entries WHERE setlist_id = ?1 ORDER BY position ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![setlist_id], |row| {
            Ok(SetlistEntry {
                id: row.get("id")?,
                setlist_id: row.get("setlist_id")?,
                track_id: row.get("track_id")?,
                position: row.get("position")?,
                transition_id: row.get("transition_id")?,
            })
        })
        .map_err(|e| e.to_string())?;
    let entries: Vec<SetlistEntry> = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let track = conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", params![entry.track_id], track_from_row)
            .map_err(|e| e.to_string())?;
        let transition = match &entry.transition_id {
            Some(tid) => conn
                .query_row("SELECT * FROM transitions WHERE id = ?1", params![tid], transition_from_row)
                .ok(),
            None => None,
        };
        result.push(SetlistEntryWithTrack { entry, track, transition });
    }
    Ok(result)
}

#[tauri::command]
pub fn add_track_to_setlist(db: State<Db>, setlist_id: String, track_id: String) -> Result<SetlistEntry, String> {
    let conn = db.0.lock().unwrap();
    let next_pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM setlist_entries WHERE setlist_id = ?1",
            params![setlist_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO setlist_entries (id, setlist_id, track_id, position) VALUES (?1, ?2, ?3, ?4)",
        params![id, setlist_id, track_id, next_pos],
    )
    .map_err(|e| e.to_string())?;
    conn.execute("UPDATE setlists SET updated_at = ?1 WHERE id = ?2", params![now(), setlist_id]).ok();
    Ok(SetlistEntry { id, setlist_id, track_id, position: next_pos, transition_id: None })
}

#[tauri::command]
pub fn remove_setlist_entry(db: State<Db>, entry_id: String) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM setlist_entries WHERE id = ?1", params![entry_id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn reorder_setlist(db: State<Db>, setlist_id: String, ordered_entry_ids: Vec<String>) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    for (i, entry_id) in ordered_entry_ids.iter().enumerate() {
        conn.execute(
            "UPDATE setlist_entries SET position = ?1 WHERE id = ?2 AND setlist_id = ?3",
            params![i as i64, entry_id, setlist_id],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute("UPDATE setlists SET updated_at = ?1 WHERE id = ?2", params![now(), setlist_id]).ok();
    Ok(())
}

#[tauri::command]
pub fn assign_transition(db: State<Db>, entry_id: String, transition_id: Option<String>) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE setlist_entries SET transition_id = ?1 WHERE id = ?2",
        params![transition_id, entry_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Transition library ----------

#[tauri::command]
pub fn save_transition(
    db: State<Db>,
    name: String,
    transition_type: String,
    crossfade_secs: f64,
    cue_out_secs: Option<f64>,
    cue_in_secs: Option<f64>,
    notes: Option<String>,
    from_key: Option<String>,
    to_key: Option<String>,
) -> Result<Transition, String> {
    let conn = db.0.lock().unwrap();
    let id = Uuid::new_v4().to_string();
    let ts = now();
    conn.execute(
        "INSERT INTO transitions (id, name, transition_type, crossfade_secs, cue_out_secs, cue_in_secs, notes, from_key, to_key, visibility, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'private', ?10)",
        params![id, name, transition_type, crossfade_secs, cue_out_secs, cue_in_secs, notes, from_key, to_key, ts],
    )
    .map_err(|e| e.to_string())?;
    conn.query_row("SELECT * FROM transitions WHERE id = ?1", params![id], transition_from_row)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_transitions(db: State<Db>) -> Result<Vec<Transition>, String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare("SELECT * FROM transitions ORDER BY created_at DESC").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], transition_from_row).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_transition(db: State<Db>, transition_id: String) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM transitions WHERE id = ?1", params![transition_id]).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Profile ----------

const LOCAL_PROFILE_ID: &str = "local";

#[tauri::command]
pub fn get_profile(db: State<Db>) -> Result<Profile, String> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO profile (id) VALUES (?1) ON CONFLICT(id) DO NOTHING",
        params![LOCAL_PROFILE_ID],
    )
    .ok();
    conn.query_row("SELECT * FROM profile WHERE id = ?1", params![LOCAL_PROFILE_ID], |row| {
        let socials_json: String = row.get("socials_json")?;
        let work_links_json: String = row.get("work_links_json")?;
        Ok(Profile {
            id: row.get("id")?,
            display_name: row.get("display_name")?,
            bio: row.get("bio")?,
            socials: serde_json::from_str(&socials_json).unwrap_or_default(),
            work_links: serde_json::from_str(&work_links_json).unwrap_or_default(),
            avatar_path: row.get("avatar_path")?,
        })
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_profile(db: State<Db>, profile: Profile) -> Result<Profile, String> {
    let socials_json = serde_json::to_string(&profile.socials).unwrap_or_else(|_| "[]".into());
    let work_links_json = serde_json::to_string(&profile.work_links).unwrap_or_else(|_| "[]".into());
    {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "UPDATE profile SET display_name = ?1, bio = ?2, socials_json = ?3, work_links_json = ?4, avatar_path = ?5 WHERE id = ?6",
            params![profile.display_name, profile.bio, socials_json, work_links_json, profile.avatar_path, LOCAL_PROFILE_ID],
        )
        .map_err(|e| e.to_string())?;
    }
    get_profile(db)
}

// ---------- Export ----------

#[tauri::command]
pub fn export_setlist(db: State<Db>, setlist_id: String, dest_folder: String) -> Result<String, String> {
    let setlist = {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT * FROM setlists WHERE id = ?1", params![setlist_id], setlist_from_row)
            .map_err(|e| e.to_string())?
    };
    let entries = get_setlist_entries(db, setlist_id)?;
    let tracks: Vec<Track> = entries.into_iter().map(|e| e.track).collect();
    if tracks.is_empty() {
        return Err("setlist has no tracks to export".to_string());
    }
    let folder = export::export_setlist_to_folder(Path::new(&dest_folder), &setlist.name, &tracks)?;
    Ok(folder.to_string_lossy().to_string())
}
