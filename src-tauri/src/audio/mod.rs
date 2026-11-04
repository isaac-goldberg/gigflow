pub mod bpm;
pub mod decode;
pub mod hotcues;
pub mod key;

use std::path::Path;

pub struct AnalysisResult {
    pub duration_secs: f64,
    pub bpm: f64,
    pub bpm_confidence: f64,
    pub key_camelot: String,
    pub key_name: String,
    pub key_confidence: f64,
    pub energy: f64,
    pub hot_cues: Vec<hotcues::HotCue>,
}

/// Full analysis pipeline for one audio file: decode -> BPM -> key -> energy -> hot cues.
pub fn analyze_file(path: &Path) -> Result<AnalysisResult, String> {
    let decoded = decode::decode_file(path)?;
    let bpm_result = bpm::detect_bpm(&decoded.samples, decoded.sample_rate);
    let key_result = key::detect_key(&decoded.samples, decoded.sample_rate);
    let energy = rms_energy(&decoded.samples);
    let hot_cues = hotcues::detect_hot_cues(&decoded.samples, decoded.sample_rate, bpm_result.bpm);

    Ok(AnalysisResult {
        duration_secs: decoded.duration_secs,
        bpm: bpm_result.bpm,
        bpm_confidence: bpm_result.confidence,
        key_camelot: key_result.camelot,
        key_name: key_result.name,
        key_confidence: key_result.confidence,
        energy,
        hot_cues,
    })
}

/// Rough 0..1 "energy" score from RMS loudness, gamma-compressed to feel more
/// perceptual. Good enough for sorting a setlist by intensity in the MVP.
fn rms_energy(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    (rms * 4.0).clamp(0.0, 1.0).powf(0.6)
}
