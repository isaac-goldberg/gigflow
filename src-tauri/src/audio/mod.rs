pub mod beatgrid;
pub mod bpm;
pub mod decode;
pub mod dsp;
#[cfg(test)]
mod diagnostic_test;
pub mod energy;
pub mod hotcues;
pub mod key;
pub mod mix;
pub mod stretch;
pub mod waveform;

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
    pub waveform_peaks: Vec<(f32, f32)>,
}

/// Full analysis pipeline for one audio file: decode -> BPM -> key -> energy -> hot cues -> waveform.
pub fn analyze_file(path: &Path) -> Result<AnalysisResult, String> {
    let decoded = decode::decode_file(path)?;
    let bpm_result = bpm::detect_bpm(&decoded.samples, decoded.sample_rate);
    let key_result = key::detect_key(&decoded.samples, decoded.sample_rate);
    let energy = energy::compute_energy(&decoded.samples, decoded.sample_rate, bpm_result.bpm);
    let hot_cues = hotcues::detect_hot_cues(&decoded.samples, decoded.sample_rate, bpm_result.bpm);
    let waveform_peaks = waveform::compute_peaks(&decoded.samples);

    Ok(AnalysisResult {
        duration_secs: decoded.duration_secs,
        bpm: bpm_result.bpm,
        bpm_confidence: bpm_result.confidence,
        key_camelot: key_result.camelot,
        key_name: key_result.name,
        key_confidence: key_result.confidence,
        energy,
        hot_cues,
        waveform_peaks,
    })
}
