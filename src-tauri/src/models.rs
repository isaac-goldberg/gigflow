use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub file_path: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
    pub bpm: Option<f64>,
    pub bpm_confidence: Option<f64>,
    pub key_camelot: Option<String>,
    pub key_name: Option<String>,
    pub key_confidence: Option<f64>,
    pub energy: Option<f64>,
    pub artwork_path: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetlistEntry {
    pub id: String,
    pub setlist_id: String,
    pub track_id: String,
    pub position: i64,
    pub transition_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetlistEntryWithTrack {
    pub entry: SetlistEntry,
    pub track: Track,
    pub transition: Option<Transition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub transition_type: String,
    pub crossfade_secs: f64,
    pub cue_out_secs: Option<f64>,
    pub cue_in_secs: Option<f64>,
    pub notes: Option<String>,
    pub from_key: Option<String>,
    pub to_key: Option<String>,
    pub visibility: String,
    pub author: Option<String>,
    pub created_at: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSuggestion {
    pub track: Track,
    pub score: f64,
    pub bpm_delta: Option<f64>,
    pub key_relation: String,
    pub energy_delta: Option<f64>,
}
