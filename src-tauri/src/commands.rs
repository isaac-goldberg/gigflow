use crate::audio;
use crate::audio::key::camelot_relation;
use crate::audio_player::{Engine, EngineStatus, Player, TransitionZone};
use std::sync::Arc;
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
    let waveform_json: Option<String> = row.get("waveform_peaks")?;
    let waveform_peaks = waveform_json.and_then(|j| serde_json::from_str(&j).ok());
    Ok(Track {
        id: row.get("id")?,
        file_path: row.get("file_path")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        genre: row.get("genre")?,
        subgenre: row.get("subgenre")?,
        duration_secs: row.get("duration_secs")?,
        bpm: row.get("bpm")?,
        bpm_confidence: row.get("bpm_confidence")?,
        key_camelot: row.get("key_camelot")?,
        key_name: row.get("key_name")?,
        key_confidence: row.get("key_confidence")?,
        energy: row.get("energy")?,
        artwork_path: row.get("artwork_path")?,
        waveform_peaks,
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

// ---------- Library ----------

#[tauri::command]
pub fn import_tracks(db: State<Db>, paths: Vec<String>) -> Result<Vec<Track>, String> {
    let conn = db.0.lock().unwrap();
    let mut imported = Vec::new();

    for path_str in paths {
        let path = PathBuf::from(&path_str);
        let tags = read_tags(&path);
        let id = Uuid::new_v4().to_string();
        let date_added = now();

        let result = conn.execute(
            "INSERT INTO tracks (id, file_path, title, artist, album, genre, subgenre, duration_secs, date_added)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(file_path) DO NOTHING",
            params![id, path_str, tags.title, tags.artist, tags.album, tags.genre, tags.subgenre, tags.duration_secs, date_added],
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

struct TagInfo {
    title: String,
    artist: Option<String>,
    album: Option<String>,
    genre: Option<String>,
    subgenre: Option<String>,
    duration_secs: Option<f64>,
}

fn read_tags(path: &Path) -> TagInfo {
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
            let raw_genre = tag.and_then(|t| t.genre()).map(|c| c.to_string()).unwrap_or_default();
            let (genre, subgenre) = crate::genre::normalize_genre(&raw_genre);
            TagInfo { title, artist, album, genre, subgenre, duration_secs: Some(duration) }
        }
        Err(_) => TagInfo { title: fallback_title, artist: None, album: None, genre: None, subgenre: None, duration_secs: None },
    }
}

#[tauri::command]
pub async fn analyze_track(
    db: State<'_, Db>,
    llm: State<'_, crate::llm::Llm>,
    track_id: String,
) -> Result<Track, String> {
    let (file_path, title, artist): (String, String, Option<String>) = {
        let conn = db.0.lock().unwrap();
        conn.query_row(
            "SELECT file_path, title, artist FROM tracks WHERE id = ?1",
            params![track_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("track not found: {e}"))?
    };

    // This is genuinely CPU-heavy (several FFT passes over the whole track).
    // Running it inline on a plain `fn` command can execute on the thread
    // handling the webview's IPC callback rather than a background thread,
    // which blocks the app's main run loop — that's the spinning-cursor
    // freeze. spawn_blocking moves it to a dedicated blocking-task thread so
    // the UI stays responsive while this runs, same as any slow command
    // should. catch_unwind is still the last line of defense so a single
    // unusual/malformed file fails *this track* with an error, never the
    // whole app (see Cargo.toml for why panic=abort is also not set).
    let file_path_for_task = file_path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            audio::analyze_file(Path::new(&file_path_for_task))
        }))
    })
    .await
    .map_err(|e| format!("analysis task failed to run: {e}"))?
    .map_err(|_| format!("analysis crashed on this file (likely an unusual or corrupt audio file): {file_path}"))??;

    {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "UPDATE tracks SET duration_secs = ?1, bpm = ?2, bpm_confidence = ?3,
             key_camelot = ?4, key_name = ?5, key_confidence = ?6, energy = ?7, waveform_peaks = ?8, date_analyzed = ?9
             WHERE id = ?10",
            params![
                result.duration_secs,
                result.bpm,
                result.bpm_confidence,
                result.key_camelot,
                result.key_name,
                result.key_confidence,
                result.energy,
                serde_json::to_string(&result.waveform_peaks).unwrap_or_else(|_| "[]".to_string()),
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
        // conn (and its MutexGuard, which isn't Send) is dropped here, before
        // the classification call below awaits across a thread hop.
    }

    // The local AI model is now the source of truth for genre/subgenre,
    // re-run on every analysis. Best-effort: if the model isn't downloaded
    // yet, or llama-server isn't installed, this fails gracefully and just
    // leaves genre/subgenre as whatever they already were (e.g. from file
    // tags at import time) rather than failing the whole analysis — BPM/
    // key/energy/hot cues above are the core feature and must not be lost
    // over an optional classifier being unavailable.
    let llm = llm.inner().clone();
    let classify_result = tauri::async_runtime::spawn_blocking(move || {
        crate::llm::classify(
            &llm,
            &crate::llm::TrackInfo {
                title: &title,
                artist: artist.as_deref(),
                bpm: Some(result.bpm),
                key_camelot: Some(result.key_camelot.as_str()),
                energy: Some(result.energy),
            },
        )
    })
    .await
    .map_err(|e| format!("classification task failed to run: {e}"))?;

    if let Ok(suggestion) = classify_result {
        let conn = db.0.lock().unwrap();
        conn.execute(
            "UPDATE tracks SET genre = ?1, subgenre = ?2 WHERE id = ?3",
            params![suggestion.genre, suggestion.subgenre, track_id],
        )
        .map_err(|e| e.to_string())?;
    }

    let conn = db.0.lock().unwrap();
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
    genre: Option<String>,
    subgenre: Option<String>,
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
    if let Some(v) = &genre {
        conn.execute("UPDATE tracks SET genre = ?1 WHERE id = ?2", params![v, track_id]).ok();
    }
    if let Some(v) = &subgenre {
        conn.execute("UPDATE tracks SET subgenre = ?1 WHERE id = ?2", params![v, track_id]).ok();
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

/// Wipes the entire library in one shot — every track, plus (via
/// `ON DELETE CASCADE`) every hot cue and setlist entry that referenced one.
#[tauri::command]
pub fn delete_all_tracks(db: State<Db>) -> Result<i64, String> {
    let conn = db.0.lock().unwrap();
    let deleted = conn.execute("DELETE FROM tracks", []).map_err(|e| e.to_string())?;
    Ok(deleted as i64)
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
                transition_type: row.get("transition_type")?,
                crossfade_secs: row.get("crossfade_secs")?,
                cue_out_secs: row.get("cue_out_secs")?,
                cue_in_secs: row.get("cue_in_secs")?,
                transition_notes: row.get("transition_notes")?,
                transition_enabled: row.get::<_, Option<i64>>("transition_enabled")?.unwrap_or(1) != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    let entries: Vec<SetlistEntry> = rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?;

    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let track = conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", params![entry.track_id], track_from_row)
            .map_err(|e| e.to_string())?;
        result.push(SetlistEntryWithTrack { entry, track });
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
    Ok(SetlistEntry {
        id,
        setlist_id,
        track_id,
        position: next_pos,
        transition_type: None,
        crossfade_secs: None,
        cue_out_secs: None,
        cue_in_secs: None,
        transition_notes: None,
        transition_enabled: true,
    })
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

/// Sets (or, passing all `None`, clears) the transition on one gap. There's
/// no separate "save" step and nothing shared across gaps — see the comment
/// on `setlist_entries` in `db.rs` for why.
#[tauri::command]
pub fn set_entry_transition(
    db: State<Db>,
    entry_id: String,
    transition_type: Option<String>,
    crossfade_secs: Option<f64>,
    cue_out_secs: Option<f64>,
    cue_in_secs: Option<f64>,
    notes: Option<String>,
) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE setlist_entries SET transition_type = ?1, crossfade_secs = ?2, cue_out_secs = ?3,
         cue_in_secs = ?4, transition_notes = ?5 WHERE id = ?6",
        params![transition_type, crossfade_secs, cue_out_secs, cue_in_secs, notes, entry_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Turns a gap's transition on/off for listening without losing its
/// settings — off means the outgoing track just plays to its end.
#[tauri::command]
pub fn set_entry_transition_enabled(db: State<Db>, entry_id: String, enabled: bool) -> Result<(), String> {
    let conn = db.0.lock().unwrap();
    conn.execute(
        "UPDATE setlist_entries SET transition_enabled = ?1 WHERE id = ?2",
        params![enabled as i64, entry_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Transition recommendation ----------

/// Picks a structure-aware starting point for blending `track_a` into
/// `track_b`, instead of leaving the DJ to guess — see `transition_suggest`
/// for the actual cue-selection and crossfade-length logic (kept separate
/// and unit-tested there, since it doesn't need a database to reason
/// about).
#[tauri::command]
pub fn suggest_transition(db: State<Db>, track_a_id: String, track_b_id: String) -> Result<TransitionSuggestion, String> {
    let conn = db.0.lock().unwrap();
    let track_a = conn
        .query_row("SELECT * FROM tracks WHERE id = ?1", params![track_a_id], track_from_row)
        .map_err(|e| format!("track A not found: {e}"))?;
    let track_b = conn
        .query_row("SELECT * FROM tracks WHERE id = ?1", params![track_b_id], track_from_row)
        .map_err(|e| format!("track B not found: {e}"))?;

    let cues_a = query_hot_cues(&conn, &track_a_id)?;
    let cues_b = query_hot_cues(&conn, &track_b_id)?;

    let out_pick = crate::transition_suggest::pick_cue_out(&cues_a);
    let in_pick = crate::transition_suggest::pick_cue_in(&cues_b);

    let clash = matches!(
        (&track_a.key_camelot, &track_b.key_camelot),
        (Some(ka), Some(kb)) if audio::key::camelot_relation(ka, kb) == "clash"
    );
    let crossfade_secs = crate::transition_suggest::recommended_crossfade_secs(
        track_a.bpm,
        clash,
        in_pick.map(|c| c.position_secs),
        track_b.duration_secs,
    );

    let out_desc = out_pick.map(|c| c.label.to_lowercase()).unwrap_or_else(|| "end".into());
    let in_desc = in_pick.map(|c| c.label.to_lowercase()).unwrap_or_else(|| "start".into());
    let bars = match track_a.bpm {
        Some(bpm) if bpm > 0.0 => crossfade_secs / (4.0 * (60.0 / bpm)),
        _ => 0.0,
    };
    let rationale = if bars > 0.0 {
        let key_note = if clash { " Keys clash, so it's kept short and the melodies are EQ'd apart." } else { "" };
        format!(
            "Blend from the {out_desc} of \"{}\" into the {in_desc} of \"{}\" over {bars:.0} bars, bass swap halfway.{key_note}",
            track_a.title, track_b.title
        )
    } else {
        format!("{out_desc} of \"{}\" into {in_desc} of \"{}\".", track_a.title, track_b.title)
    };

    Ok(TransitionSuggestion {
        transition_type: "crossfade".to_string(),
        crossfade_secs,
        cue_out_secs: out_pick.map(|c| c.position_secs),
        cue_in_secs: in_pick.map(|c| c.position_secs),
        rationale,
    })
}

fn query_hot_cues(conn: &rusqlite::Connection, track_id: &str) -> Result<Vec<HotCue>, String> {
    let mut stmt = conn
        .prepare("SELECT * FROM hot_cues WHERE track_id = ?1 ORDER BY position_secs ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![track_id], hot_cue_from_row).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
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
        conn.query_row("SELECT * FROM setlists WHERE id = ?1", params![setlist_id.clone()], setlist_from_row)
            .map_err(|e| e.to_string())?
    };
    let setlist_entries = get_setlist_entries(db.clone(), setlist_id)?;
    if setlist_entries.is_empty() {
        return Err("setlist has no tracks to export".to_string());
    }

    let export_entries: Vec<ExportEntry> = {
        let conn = db.0.lock().unwrap();
        setlist_entries
            .into_iter()
            .map(|e| {
                let mut stmt = conn
                    .prepare("SELECT * FROM hot_cues WHERE track_id = ?1 ORDER BY position_secs ASC")
                    .unwrap();
                let hot_cues: Vec<HotCue> = stmt
                    .query_map(params![e.track.id], hot_cue_from_row)
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                ExportEntry {
                    track: e.track,
                    hot_cues,
                    transition_type: e.entry.transition_type,
                    crossfade_secs: e.entry.crossfade_secs,
                    cue_out_secs: e.entry.cue_out_secs,
                    cue_in_secs: e.entry.cue_in_secs,
                }
            })
            .collect()
    };

    let folder = export::export_setlist_to_folder(Path::new(&dest_folder), &setlist.name, &export_entries)?;
    Ok(folder.to_string_lossy().to_string())
}

// ---------- Genre cleanup & bulk maintenance ----------

/// Re-normalizes every track's existing genre value in place (for tracks
/// imported before the cleanup rules existed, or whose tag was edited
/// since). Does not touch tracks with no genre at all.
#[tauri::command]
pub fn normalize_all_genres(db: State<Db>) -> Result<i64, String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, genre FROM tracks WHERE genre IS NOT NULL AND genre != ''")
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let mut changed = 0i64;
    for (id, raw_genre) in rows {
        let (genre, subgenre) = crate::genre::normalize_genre(&raw_genre);
        conn.execute(
            "UPDATE tracks SET genre = ?1, subgenre = COALESCE(subgenre, ?2) WHERE id = ?3",
            params![genre, subgenre, id],
        )
        .map_err(|e| e.to_string())?;
        changed += 1;
    }
    Ok(changed)
}

/// A rough BPM-based EDM subgenre suggestion for one track — a guess to
/// offer the user, never auto-applied. Returns None when BPM isn't in a
/// range specific enough to say anything useful, or the track isn't EDM.
#[tauri::command]
pub fn suggest_subgenre(db: State<Db>, track_id: String) -> Result<Option<String>, String> {
    let conn = db.0.lock().unwrap();
    let track = conn
        .query_row("SELECT * FROM tracks WHERE id = ?1", params![track_id], track_from_row)
        .map_err(|e| e.to_string())?;
    if track.genre.as_deref() != Some("EDM") {
        return Ok(None);
    }
    let Some(bpm) = track.bpm else { return Ok(None) };
    Ok(crate::genre::suggest_edm_subgenre_from_bpm(bpm).map(|s| s.to_string()))
}

// ---------- AI genre classification (local LLM) ----------

#[tauri::command]
pub fn genre_model_downloaded() -> bool {
    crate::llm::model_downloaded()
}

/// Downloads the ~2.5GB local model — only ever called from an explicit
/// user action, never automatically. spawn_blocking because it's a
/// multi-minute network+disk operation.
#[tauri::command]
pub async fn download_genre_model() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::llm::download_model)
        .await
        .map_err(|e| format!("download task failed to run: {e}"))?
}

/// Clears analysis (BPM/key/energy/hot cues) from every track so "Analyze
/// all" can be re-run from a clean slate — tracks and their metadata stay,
/// nothing is deleted from the library.
#[tauri::command]
pub fn reset_all_analysis(db: State<Db>) -> Result<i64, String> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM hot_cues", []).map_err(|e| e.to_string())?;
    let changed = conn
        .execute(
            "UPDATE tracks SET bpm = NULL, bpm_confidence = NULL, key_camelot = NULL,
             key_name = NULL, key_confidence = NULL, energy = NULL, date_analyzed = NULL",
            [],
        )
        .map_err(|e| e.to_string())?;
    Ok(changed as i64)
}

// ---------- Playback (live two-deck mix engine) ----------

fn decode_for_engine(file_path: String, target_sr: Option<u32>) -> Result<(Vec<f32>, u32), String> {
    let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| audio::decode::decode_file(Path::new(&file_path))))
        .map_err(|_| "decoding crashed (likely an unusual or corrupt audio file)".to_string())??;
    match target_sr {
        Some(sr) if sr != decoded.sample_rate => Ok((audio::decode::resample_linear(&decoded.samples, decoded.sample_rate, sr), sr)),
        _ => Ok((decoded.samples, decoded.sample_rate)),
    }
}

/// Starts a track on the main deck (replacing whatever was playing).
/// Transitions are armed separately with `arm_transition`, so the same call
/// serves the library (plain playback) and setlists (continuous mix).
#[tauri::command]
pub async fn play_track(db: State<'_, Db>, player: State<'_, Player>, track_id: String) -> Result<f64, String> {
    let file_path: String = {
        let conn = db.0.lock().unwrap();
        conn.query_row("SELECT file_path FROM tracks WHERE id = ?1", params![track_id], |r| r.get(0))
            .map_err(|e| format!("track not found: {e}"))?
    };
    let (samples, sr) = tauri::async_runtime::spawn_blocking(move || decode_for_engine(file_path, None))
        .await
        .map_err(|e| format!("playback task failed to run: {e}"))??;
    let duration_secs = samples.len() as f64 / sr as f64;
    let samples = Arc::new(samples);
    player.cache_decode(&track_id, sr, samples.clone());
    let deck = crate::audio_player::Deck::new(track_id, samples, sr, 0.0);
    player.start(Engine::new(sr, deck))?;
    Ok(duration_secs)
}

/// Prepares the transition from the track currently on the main deck into
/// `to_track_id`: decodes B, fits beat grids at both mix points, renders B's
/// beatmatched head, and hands it to the engine, which brings B in live when
/// A's playhead reaches the mix point. Returns the exact transition zone.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn arm_transition(
    db: State<'_, Db>,
    player: State<'_, Player>,
    from_track_id: String,
    to_track_id: String,
    transition_type: String,
    crossfade_secs: Option<f64>,
    cue_out_secs: Option<f64>,
    cue_in_secs: Option<f64>,
) -> Result<Option<TransitionZone>, String> {
    let engine = player.engine().ok_or("nothing is playing")?;
    let (main_id, a_samples, sr, seq) = {
        let e = engine.lock().unwrap();
        let (id, samples) = e.main_samples().ok_or("nothing is playing")?;
        (id, samples, e.sr, e.arm_seq)
    };
    if main_id != from_track_id {
        return Ok(None);
    }
    let (track_a, track_b) = {
        let conn = db.0.lock().unwrap();
        let a = conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", params![from_track_id], track_from_row)
            .map_err(|e| format!("track A not found: {e}"))?;
        let b = conn
            .query_row("SELECT * FROM tracks WHERE id = ?1", params![to_track_id], track_from_row)
            .map_err(|e| format!("track B not found: {e}"))?;
        (a, b)
    };

    let b_samples = match player.cached_decode(&to_track_id, sr) {
        Some(s) => s,
        None => {
            let path = track_b.file_path.clone();
            let (s, _) = tauri::async_runtime::spawn_blocking(move || decode_for_engine(path, Some(sr)))
                .await
                .map_err(|e| format!("decode task failed to run: {e}"))??;
            let s = Arc::new(s);
            player.cache_decode(&to_track_id, sr, s.clone());
            s
        }
    };

    let b_for_plan = b_samples.clone();
    let plan = tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let req = audio::mix::Request {
                kind: audio::mix::Kind::parse(&transition_type),
                crossfade_secs,
                cue_out_secs,
                cue_in_secs,
            };
            audio::mix::plan(
                audio::mix::TrackInfo { samples: &a_samples, bpm: track_a.bpm, key: track_a.key_camelot.as_deref() },
                audio::mix::TrackInfo { samples: &b_for_plan, bpm: track_b.bpm, key: track_b.key_camelot.as_deref() },
                sr,
                &req,
            )
        }))
    })
    .await
    .map_err(|e| format!("transition task failed to run: {e}"))?
    .map_err(|_| "preparing the transition crashed (likely an unusual audio file)".to_string())?;

    // Still the same playback session, same track, and no newer arm/disarm
    // happened while we were working?
    let current = player.engine().ok_or("playback stopped")?;
    if !Arc::ptr_eq(&current, &engine) {
        return Ok(None);
    }
    let mut e = engine.lock().unwrap();
    if e.arm_seq != seq || e.main.as_ref().map(|m| m.track_id.as_str()) != Some(from_track_id.as_str()) {
        return Ok(None);
    }
    let zone = crate::audio_player::zone_of(&plan, &from_track_id, &to_track_id, sr);
    e.arm(Arc::new(plan), to_track_id, b_samples);
    Ok(Some(zone))
}

#[tauri::command]
pub fn disarm_transition(player: State<Player>) -> Result<(), String> {
    if let Some(engine) = player.engine() {
        engine.lock().unwrap().disarm();
    }
    Ok(())
}

#[tauri::command]
pub fn pause_track(player: State<Player>) -> Result<(), String> {
    player.pause();
    Ok(())
}

#[tauri::command]
pub fn resume_track(player: State<Player>) -> Result<(), String> {
    player.resume();
    Ok(())
}

/// Seeks one deck: the incoming track mid-blend (which ends the blend there)
/// or otherwise the main deck.
#[tauri::command]
pub fn seek_track(player: State<Player>, track_id: Option<String>, position_secs: f64) -> Result<(), String> {
    if let Some(engine) = player.engine() {
        engine.lock().unwrap().seek(track_id.as_deref(), position_secs.max(0.0));
    }
    Ok(())
}

#[tauri::command]
pub fn get_playback_status(player: State<Player>) -> Result<Option<EngineStatus>, String> {
    let paused = player.is_paused();
    Ok(player.engine().map(|e| e.lock().unwrap().status(paused)))
}

#[tauri::command]
pub fn stop_track(player: State<Player>) -> Result<(), String> {
    player.stop()
}
