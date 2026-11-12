mod audio;
mod audio_player;
mod commands;
mod db;
mod export;
mod genre;
mod llm;
mod models;
mod transition_suggest;

/// Even with catch_unwind guarding the known analysis entry points, record
/// every panic to a log file. In a packaged GUI app stderr goes nowhere
/// visible, so without this, a crash we didn't anticipate leaves no trail at
/// all to debug from.
fn install_panic_logger() {
    let log_path = db::app_data_dir().join("crash.log");
    std::panic::set_hook(Box::new(move |info| {
        let msg = match info.payload().downcast_ref::<&str>() {
            Some(s) => s.to_string(),
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => s.clone(),
                None => "<non-string panic payload>".to_string(),
            },
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());
        let entry = format!("[{}] panic at {}: {}\n", chrono::Utc::now().to_rfc3339(), location, msg);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            use std::io::Write;
            let _ = f.write_all(entry.as_bytes());
        }
        eprintln!("{entry}");
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_panic_logger();
    let player = audio_player::Player::spawn();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(db::init())
        .manage(player)
        .manage(llm::Llm::new())
        .invoke_handler(tauri::generate_handler![
            commands::import_tracks,
            commands::analyze_track,
            commands::list_tracks,
            commands::update_track,
            commands::delete_track,
            commands::delete_all_tracks,
            commands::get_hot_cues,
            commands::delete_hot_cue,
            commands::suggest_next_tracks,
            commands::create_setlist,
            commands::list_setlists,
            commands::delete_setlist,
            commands::get_setlist_entries,
            commands::add_track_to_setlist,
            commands::remove_setlist_entry,
            commands::reorder_setlist,
            commands::set_entry_transition,
            commands::suggest_transition,
            commands::get_profile,
            commands::update_profile,
            commands::export_setlist,
            commands::arm_transition,
            commands::disarm_transition,
            commands::set_entry_transition_enabled,
            commands::normalize_all_genres,
            commands::suggest_subgenre,
            commands::genre_model_downloaded,
            commands::download_genre_model,
            commands::reset_all_analysis,
            commands::play_track,
            commands::pause_track,
            commands::resume_track,
            commands::seek_track,
            commands::get_playback_status,
            commands::stop_track,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
