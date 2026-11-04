use crate::models::Track;
use std::path::{Path, PathBuf};

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn file_uri(path: &str) -> String {
    let encoded = path
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '#' => "%23".to_string(),
            '?' => "%3F".to_string(),
            _ => c.to_string(),
        })
        .collect::<String>();
    format!("file://localhost{}", encoded)
}

/// Build a Rekordbox-compatible collection+playlist XML for a setlist.
/// This is the documented interchange format (Rekordbox: File > Export Collection
/// in xml format / Import Playlist) that Rekordbox, VirtualDJ and Engine DJ can
/// all read, without touching any proprietary USB database format.
pub fn build_rekordbox_xml(setlist_name: &str, tracks: &[Track]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n");
    out.push_str("<DJ_PLAYLISTS Version=\"1.0.0\">\n");
    out.push_str("  <PRODUCT Name=\"Gigflow\" Version=\"0.1.0\" Company=\"Gigflow\"/>\n");
    out.push_str(&format!("  <COLLECTION Entries=\"{}\">\n", tracks.len()));

    for (i, t) in tracks.iter().enumerate() {
        let id = i + 1;
        let total_time = t.duration_secs.unwrap_or(0.0).round() as i64;
        let bpm = t.bpm.unwrap_or(0.0);
        let key = t.key_name.clone().unwrap_or_default();
        let comment = format!(
            "Camelot: {}",
            t.key_camelot.clone().unwrap_or_else(|| "?".to_string())
        );
        out.push_str(&format!(
            "    <TRACK TrackID=\"{id}\" Name=\"{name}\" Artist=\"{artist}\" Album=\"{album}\" Kind=\"Audio File\" TotalTime=\"{total_time}\" Location=\"{loc}\" AverageBpm=\"{bpm:.2}\" Tonality=\"{key}\" Comments=\"{comment}\"/>\n",
            id = id,
            name = xml_escape(&t.title),
            artist = xml_escape(t.artist.as_deref().unwrap_or("")),
            album = xml_escape(t.album.as_deref().unwrap_or("")),
            total_time = total_time,
            loc = file_uri(&t.file_path),
            bpm = bpm,
            key = xml_escape(&key),
            comment = xml_escape(&comment),
        ));
    }
    out.push_str("  </COLLECTION>\n");
    out.push_str("  <PLAYLISTS>\n");
    out.push_str("    <NODE Type=\"0\" Name=\"ROOT\" Count=\"1\">\n");
    out.push_str(&format!(
        "      <NODE Name=\"{}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">\n",
        xml_escape(setlist_name),
        tracks.len()
    ));
    for i in 0..tracks.len() {
        out.push_str(&format!("        <TRACK Key=\"{}\"/>\n", i + 1));
    }
    out.push_str("      </NODE>\n");
    out.push_str("    </NODE>\n");
    out.push_str("  </PLAYLISTS>\n");
    out.push_str("</DJ_PLAYLISTS>\n");
    out
}

/// Build a simple extended-M3U playlist, readable by nearly every CDJ, media
/// player and DJ app as a generic fallback.
pub fn build_m3u8(tracks: &[Track]) -> String {
    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    for t in tracks {
        let duration = t.duration_secs.unwrap_or(0.0).round() as i64;
        let artist = t.artist.clone().unwrap_or_default();
        out.push_str(&format!("#EXTINF:{duration},{artist} - {title}\n", title = t.title));
        out.push_str(&t.file_path);
        out.push('\n');
    }
    out
}

/// Export a setlist to a destination folder (e.g. a mounted USB drive):
/// copies every audio file alongside a Rekordbox XML and an M3U8 playlist.
pub fn export_setlist_to_folder(
    dest_root: &Path,
    setlist_name: &str,
    tracks: &[Track],
) -> Result<PathBuf, String> {
    let safe_name: String = setlist_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let folder = dest_root.join(format!("Gigflow - {safe_name}"));
    std::fs::create_dir_all(&folder).map_err(|e| format!("could not create export folder: {e}"))?;

    let music_folder = folder.join("tracks");
    std::fs::create_dir_all(&music_folder).map_err(|e| format!("could not create tracks folder: {e}"))?;

    let mut copied_tracks: Vec<Track> = Vec::with_capacity(tracks.len());
    for t in tracks {
        let src = Path::new(&t.file_path);
        let file_name = src.file_name().ok_or_else(|| "track has no file name".to_string())?;
        let dest = music_folder.join(file_name);
        std::fs::copy(src, &dest).map_err(|e| format!("could not copy {}: {e}", t.title))?;
        let mut copy = t.clone();
        copy.file_path = dest.to_string_lossy().to_string();
        copied_tracks.push(copy);
    }

    let xml = build_rekordbox_xml(setlist_name, &copied_tracks);
    std::fs::write(folder.join("rekordbox.xml"), xml).map_err(|e| format!("could not write rekordbox.xml: {e}"))?;

    let m3u = build_m3u8(&copied_tracks);
    std::fs::write(folder.join("playlist.m3u8"), m3u).map_err(|e| format!("could not write playlist.m3u8: {e}"))?;

    Ok(folder)
}
