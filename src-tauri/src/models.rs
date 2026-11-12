use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub file_path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub subgenre: Option<String>,
    pub duration_secs: Option<f64>,
    pub bpm: Option<f64>,
    pub bpm_confidence: Option<f64>,
    pub key_camelot: Option<String>,
    pub key_name: Option<String>,
    pub key_confidence: Option<f64>,
    pub energy: Option<f64>,
    pub artwork_path: Option<String>,
    /// (min, max) amplitude per downsampled time bucket, for drawing a real
    /// waveform — `None` until the track has been analyzed.
    pub waveform_peaks: Option<Vec<(f32, f32)>>,
    pub date_added: String,
    pub date_analyzed: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setlist {
    pub id: String,
    pub name: String,
    pub gig_date: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The transition fields are inline here, not a separate shared/"saved"
/// entity: a transition's cue points only ever make sense for the specific
/// pair of tracks on either side of this one gap, so there's nothing to
/// reuse elsewhere by giving it its own identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetlistEntry {
    pub id: String,
    pub setlist_id: String,
    pub track_id: String,
    pub position: i64,
    pub transition_type: Option<String>,
    pub crossfade_secs: Option<f64>,
    pub cue_out_secs: Option<f64>,
    pub cue_in_secs: Option<f64>,
    pub transition_notes: Option<String>,
    /// Off = keep the transition's settings but play the outgoing track to
    /// its end when listening.
    pub transition_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetlistEntryWithTrack {
    pub entry: SetlistEntry,
    pub track: Track,
}

/// What `suggest_transition` proposes for a track pair: a phrase-aligned,
/// structure-aware starting point the user can take as-is or adjust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionSuggestion {
    pub transition_type: String,
    pub crossfade_secs: f64,
    pub cue_out_secs: Option<f64>,
    pub cue_in_secs: Option<f64>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub display_name: String,
    pub bio: String,
    pub socials: Vec<SocialLink>,
    pub work_links: Vec<WorkLink>,
    pub avatar_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLink {
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkLink {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotCue {
    pub id: String,
    pub track_id: String,
    pub pad: i64,
    pub position_secs: f64,
    pub label: String,
}

/// One setlist slot for export: the track, its stored hot cues, and the
/// transition (if any) assigned to blend into the *next* entry.
#[derive(Debug, Clone)]
pub struct ExportEntry {
    pub track: Track,
    pub hot_cues: Vec<HotCue>,
    pub transition_type: Option<String>,
    pub crossfade_secs: Option<f64>,
    pub cue_out_secs: Option<f64>,
    pub cue_in_secs: Option<f64>,
}

/// What the local AI genre classifier (`llm.rs`) proposes for one track —
/// always a guess to confirm or dismiss, never auto-applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiGenreSuggestion {
    pub genre: String,
    pub subgenre: Option<String>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSuggestion {
    pub track: Track,
    pub score: f64,
    pub bpm_delta: Option<f64>,
    pub key_relation: String,
    pub energy_delta: Option<f64>,
}
