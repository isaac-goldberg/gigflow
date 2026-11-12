/// Cleans up the messy, inconsistent genre tags typical of record-pool/
/// distributor MP3s (compound strings mixing genre, mood and market labels —
/// e.g. "Dance Commercial/Mainstream Club", "Dance/Vocal/Club") into a small
/// canonical genre + subgenre pair.
///
/// This is NOT audio-based genre classification — it's a deterministic
/// cleanup of whatever the file's own tag already claims. True genre
/// classification from audio alone is a real ML problem (trained on
/// thousands of labeled tracks) that a hand-rolled rule can't responsibly
/// claim to solve; see the BPM-based subgenre *hint* below for the one place
/// we do make a measured-data guess, clearly as a guess.
///
/// Tokens are matched by priority: specific electronic subgenres first, then
/// hip-hop/R&B, then pop, with generic "dance"/"club"/"vocal"/"mainstream"/
/// "commercial" tokens treated as non-genre filler and ignored. An
/// unrecognized raw tag is left untouched rather than discarded, since we'd
/// rather keep possibly-useful unknown data than silently blank it.
pub fn normalize_genre(raw: &str) -> (Option<String>, Option<String>) {
    let raw = raw.trim();
    if raw.is_empty() {
        return (None, None);
    }

    let tokens: Vec<String> = raw
        .split(|c: char| c == '/' || c == ',' || c == '-' || c == '&')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    let joined = tokens.join(" ");
    let has = |needle: &str| joined.contains(needle);

    // Most specific EDM subgenre first.
    let edm_subgenre = if has("drum") && has("bass") {
        Some("Drum & Bass")
    } else if has("dubstep") {
        Some("Dubstep")
    } else if has("trap") {
        Some("Trap")
    } else if has("techno") {
        Some("Techno")
    } else if has("trance") {
        Some("Trance")
    } else if has("big room") {
        Some("Big Room House")
    } else if has("progressive") {
        Some("Progressive House")
    } else if has("house") {
        Some("House")
    } else if has("electro") {
        Some("Electro")
    } else {
        None
    };
    if let Some(sub) = edm_subgenre {
        return (Some("EDM".to_string()), Some(sub.to_string()));
    }

    if has("hip-hop") || has("hip hop") || has("rap") || has("r&b") || has("rnb") || has("urban") {
        return (Some("Hip-Hop/R&B".to_string()), None);
    }

    // In DJ record-pool taxonomy, "Mainstream"/"Commercial" (as a category,
    // not just a stray adjective) specifically means Top 40/radio-leaning
    // pop-dance, as distinct from specialist EDM/urban/open-format
    // categories — so on its own it's a real (if coarse) signal, not noise.
    if has("pop") || has("top 40") || has("mainstream") || has("commercial") {
        return (Some("Pop".to_string()), None);
    }

    if has("rock") {
        return (Some("Rock".to_string()), None);
    }

    // Generic "dance"/"club"/"vocal"/"mainstream"/"commercial" tokens, alone
    // or in any combination, aren't specific enough to claim more than this.
    if has("dance") || has("club") {
        return (Some("Dance".to_string()), None);
    }

    // Didn't recognize any token (e.g. a label/market string we don't have a
    // rule for) — keep the original rather than lose information.
    (Some(raw.to_string()), None)
}

/// The fixed taxonomy the local AI genre classifier (`llm.rs`) is constrained
/// to choose from via grammar-constrained JSON output — never free text, so
/// its guesses always line up with something meaningful in this app rather
/// than an arbitrary string the model invented.
pub const LLM_GENRES: &[&str] = &[
    "EDM",
    "Hip-Hop/R&B",
    "Pop",
    "Rock",
    "Metal",
    "Dance",
    "Latin",
    "Afrobeats",
    "Reggae/Dancehall",
    "Country",
    "Folk",
    "Jazz",
    "Classical",
    "Funk/Soul",
    "Blues",
    "World",
    "Indie/Alternative",
    "Other",
];

/// Only meaningful (non-null) when the chosen genre is "EDM" — see the
/// system prompt in `llm.rs`. Several of these (House, Progressive House,
/// Big Room House, Electro, Bass House, Tech House, Afro House) commonly
/// share the same ~118-128 BPM range, so BPM alone can't separate them —
/// the model is explicitly told not to use "House" as a default/safe guess
/// just because a track lands in that range.
pub const LLM_EDM_SUBGENRES: &[&str] = &[
    "House",
    "Tech House",
    "Deep House",
    "Bass House",
    "Afro House",
    "Progressive House",
    "Big Room House",
    "Electro",
    "Techno",
    "Melodic Techno",
    "Trance",
    "Dubstep",
    "Drum & Bass",
    "Trap",
    "Future Bass",
    "Hardstyle",
    "UK Garage/Bassline",
];

/// A rough, clearly-labeled EDM subgenre *hint* from BPM alone, for tracks
/// that normalized to "EDM" with no subgenre from the tag itself. BPM is a
/// weak signal on its own (house/techno/trance ranges heavily overlap), so
/// this only fires where the tempo is reasonably diagnostic, and the caller
/// must present it as a suggestion to confirm, never as an asserted fact.
pub fn suggest_edm_subgenre_from_bpm(bpm: f64) -> Option<&'static str> {
    match bpm {
        b if b >= 160.0 && b <= 185.0 => Some("Drum & Bass"),
        b if b >= 135.0 && b < 145.0 => Some("Dubstep"),
        b if b >= 128.0 && b < 135.0 => Some("Techno / Trance"),
        b if b >= 118.0 && b < 128.0 => Some("House"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_real_library_tags() {
        assert_eq!(normalize_genre("Big Room"), (Some("EDM".into()), Some("Big Room House".into())));
        assert_eq!(normalize_genre("Electro/Progressive"), (Some("EDM".into()), Some("Progressive House".into())));
        assert_eq!(normalize_genre("House"), (Some("EDM".into()), Some("House".into())));
        assert_eq!(normalize_genre("Hip-Hop / R&B"), (Some("Hip-Hop/R&B".into()), None));
        assert_eq!(normalize_genre("Urban"), (Some("Hip-Hop/R&B".into()), None));
        assert_eq!(normalize_genre("Mainstream"), (Some("Pop".into()), None));
        assert_eq!(normalize_genre("Top 40"), (Some("Pop".into()), None));
        assert_eq!(normalize_genre("Dance Commercial/Mainstream Club"), (Some("Pop".into()), None));
        assert_eq!(normalize_genre("Dance/Vocal/Club"), (Some("Dance".into()), None));
        assert_eq!(normalize_genre("Club/Vocal/Dance"), (Some("Dance".into()), None));
        assert_eq!(normalize_genre("Dance / Electro Pop"), (Some("EDM".into()), Some("Electro".into())));
        assert_eq!(normalize_genre(""), (None, None));
        assert_eq!(normalize_genre("Some Weird Label"), (Some("Some Weird Label".into()), None));
    }

    #[test]
    fn bpm_hints_only_fire_in_diagnostic_ranges() {
        assert_eq!(suggest_edm_subgenre_from_bpm(174.0), Some("Drum & Bass"));
        assert_eq!(suggest_edm_subgenre_from_bpm(140.0), Some("Dubstep"));
        assert_eq!(suggest_edm_subgenre_from_bpm(122.0), Some("House"));
        assert_eq!(suggest_edm_subgenre_from_bpm(100.0), None);
    }
}
