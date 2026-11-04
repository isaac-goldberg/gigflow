mod audio;
mod commands;
mod db;
mod export;
mod models;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(db::init())
        .invoke_handler(tauri::generate_handler![
            commands::import_tracks,
            commands::analyze_track,
            commands::list_tracks,
            commands::update_track,
            commands::delete_track,
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
            commands::assign_transition,
            commands::save_transition,
            commands::list_transitions,
            commands::delete_transition,
            commands::get_profile,
            commands::update_profile,
            commands::export_setlist,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
