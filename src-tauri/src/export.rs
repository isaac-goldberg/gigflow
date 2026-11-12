use crate::models::ExportEntry;
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
///
/// Rekordbox has no concept of two playlist tracks being pre-blended into
/// one — that's the DJ's live mixing job, same as on a real CDJ pair. What we
/// *can* do is place the automated hot cues, plus explicit "mix out" / "mix
/// in" memory cues at each assigned transition's boundary, as real
/// <POSITION_MARK> entries Rekordbox reads onto its own hot cue pads.
pub fn build_rekordbox_xml(setlist_name: &str, entries: &[ExportEntry]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\n");
    out.push_str("<DJ_PLAYLISTS Version=\"1.0.0\">\n");
    out.push_str("  <PRODUCT Name=\"Gigflow\" Version=\"0.1.0\" Company=\"Gigflow\"/>\n");
    out.push_str(&format!("  <COLLECTION Entries=\"{}\">\n", entries.len()));

    for (i, entry) in entries.iter().enumerate() {
        let t = &entry.track;
        let id = i + 1;
        let total_time = t.duration_secs.unwrap_or(0.0).round() as i64;
        let bpm = t.bpm.unwrap_or(0.0);
        let key = t.key_name.clone().unwrap_or_default();
        let comment = format!(
            "Camelot: {}",
            t.key_camelot.clone().unwrap_or_else(|| "?".to_string())
        );
        // Rekordbox's XML has one Genre field and no subgenre concept, so
        // fold both into it ("House - Tech House") rather than dropping the
        // subgenre on export.
        let genre = match (&t.genre, &t.subgenre) {
            (Some(g), Some(sg)) if !g.is_empty() && !sg.is_empty() => format!("{g} - {sg}"),
            (Some(g), _) if !g.is_empty() => g.clone(),
            (_, Some(sg)) if !sg.is_empty() => sg.clone(),
            _ => String::new(),
        };

        // Gather this track's position marks: its automated hot cues, a
        // "mix out" mark where it transitions into the next track, and a
        // "mix in" mark if the *previous* track's transition points here.
        let mut marks: Vec<(f64, String)> = entry
            .hot_cues
            .iter()
            .map(|c| (c.position_secs, c.label.clone()))
            .collect();

        if let Some(ty) = &entry.transition_type {
            let crossfade = entry.crossfade_secs.unwrap_or(8.0);
            let mix_out_at = entry
                .cue_out_secs
                .unwrap_or_else(|| (t.duration_secs.unwrap_or(0.0) - crossfade).max(0.0));
            marks.push((mix_out_at, format!("MIX OUT -> {}", ty.to_uppercase())));
        }
        if i > 0 {
            if let Some(ty) = &entries[i - 1].transition_type {
                let mix_in_at = entries[i - 1].cue_in_secs.unwrap_or(0.0);
                marks.push((mix_in_at, format!("MIX IN <- {}", ty.to_uppercase())));
            }
        }
        marks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        marks.truncate(8); // Rekordbox hot cue pads: A-H / 1-8

        out.push_str(&format!(
            "    <TRACK TrackID=\"{id}\" Name=\"{name}\" Artist=\"{artist}\" Album=\"{album}\" Genre=\"{genre}\" Kind=\"Audio File\" TotalTime=\"{total_time}\" Location=\"{loc}\" AverageBpm=\"{bpm:.2}\" Tonality=\"{key}\" Comments=\"{comment}\">\n",
            id = id,
            name = xml_escape(&t.title),
            artist = xml_escape(t.artist.as_deref().unwrap_or("")),
            album = xml_escape(t.album.as_deref().unwrap_or("")),
            genre = xml_escape(&genre),
            total_time = total_time,
            loc = file_uri(&t.file_path),
            bpm = bpm,
            key = xml_escape(&key),
            comment = xml_escape(&comment),
        ));
        for (num, (position, label)) in marks.iter().enumerate() {
            out.push_str(&format!(
                "      <POSITION_MARK Name=\"{name}\" Type=\"0\" Start=\"{start:.3}\" Num=\"{num}\"/>\n",
                name = xml_escape(label),
                start = position,
                num = num,
            ));
        }
        out.push_str("    </TRACK>\n");
    }
    out.push_str("  </COLLECTION>\n");
    out.push_str("  <PLAYLISTS>\n");
    out.push_str("    <NODE Type=\"0\" Name=\"ROOT\" Count=\"1\">\n");
    out.push_str(&format!(
        "      <NODE Name=\"{}\" Type=\"1\" KeyType=\"0\" Entries=\"{}\">\n",
        xml_escape(setlist_name),
        entries.len()
    ));
    for i in 0..entries.len() {
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
pub fn build_m3u8(entries: &[ExportEntry]) -> String {
    let mut out = String::new();
    out.push_str("#EXTM3U\n");
    for entry in entries {
        let t = &entry.track;
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
    entries: &[ExportEntry],
) -> Result<PathBuf, String> {
    let safe_name: String = setlist_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let folder = dest_root.join(format!("Gigflow - {safe_name}"));
    std::fs::create_dir_all(&folder).map_err(|e| format!("could not create export folder: {e}"))?;

    let music_folder = folder.join("tracks");
    std::fs::create_dir_all(&music_folder).map_err(|e| format!("could not create tracks folder: {e}"))?;

    let mut copied: Vec<ExportEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        let src = Path::new(&entry.track.file_path);
        let file_name = src.file_name().ok_or_else(|| "track has no file name".to_string())?;
        let dest = music_folder.join(file_name);
        std::fs::copy(src, &dest).map_err(|e| format!("could not copy {}: {e}", entry.track.title))?;
        let mut copy = entry.clone();
        copy.track.file_path = dest.to_string_lossy().to_string();
        copied.push(copy);
    }

    let xml = build_rekordbox_xml(setlist_name, &copied);
    std::fs::write(folder.join("rekordbox.xml"), xml).map_err(|e| format!("could not write rekordbox.xml: {e}"))?;

    let m3u = build_m3u8(&copied);
    std::fs::write(folder.join("playlist.m3u8"), m3u).map_err(|e| format!("could not write playlist.m3u8: {e}"))?;

    Ok(folder)
}
