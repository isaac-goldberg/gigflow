use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

// Hard cap on how much audio we ever decode into memory for one file. Plenty
// for any real song or extended DJ edit; protects against something
// pathological ending up in the library (a multi-hour recording imported by
// mistake, a corrupt file symphonia mis-parses) eating gigabytes of RAM and
// aborting the whole process — not a panic `catch_unwind` can save you from.
const MAX_DECODE_SECS: f64 = 20.0 * 60.0;

pub struct DecodedAudio {
    pub samples: Vec<f32>, // mono, interleaved-down-to-mono
    pub sample_rate: u32,
    pub duration_secs: f64,
}

/// Decode an audio file (mp3, wav, flac, ogg/vorbis, aac/m4a, aiff, alac...) to mono f32 samples.
pub fn decode_file(path: &Path) -> Result<DecodedAudio, String> {
    let file = File::open(path).map_err(|e| format!("could not open file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .map_err(|e| format!("unsupported or unreadable audio file: {e}"))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.channels.is_some())
        .ok_or_else(|| "no decodable audio track found".to_string())?
        .clone();

    // The container header often states total duration directly (frame
    // count + time base), with no need to decode anything — use that for
    // the *reported* duration so a capped decode below doesn't make a long
    // file look artificially short in the UI.
    let container_duration_secs = match (track.codec_params.n_frames, track.codec_params.time_base) {
        (Some(n_frames), Some(tb)) => Some(n_frames as f64 * tb.numer as f64 / tb.denom as f64),
        _ => None,
    };

    let track_id = track.id;
    let dec_opts: DecoderOptions = Default::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .map_err(|e| format!("could not create decoder: {e}"))?;

    let mut sample_rate: u32 = track.codec_params.sample_rate.unwrap_or(44100);
    let mut mono_samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(format!("error reading packet: {e}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec: SignalSpec = *decoded.spec();
                sample_rate = spec.rate;
                let channels = spec.channels.count().max(1);

                if sample_buf.is_none() {
                    let duration = decoded.capacity() as u64;
                    sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                }
                let buf = sample_buf.as_mut().unwrap();
                buf.copy_interleaved_ref(decoded);
                let data = buf.samples();

                // Downmix to mono.
                if channels == 1 {
                    mono_samples.extend_from_slice(data);
                } else {
                    for frame in data.chunks_exact(channels) {
                        let sum: f32 = frame.iter().sum();
                        mono_samples.push(sum / channels as f32);
                    }
                }

                if mono_samples.len() as f64 >= MAX_DECODE_SECS * sample_rate as f64 {
                    break;
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(format!("decode error: {e}")),
        }
    }

    if mono_samples.is_empty() {
        return Err("decoded zero samples from file".to_string());
    }

    let decoded_secs = mono_samples.len() as f64 / sample_rate as f64;
    let duration_secs = container_duration_secs.unwrap_or(decoded_secs);

    Ok(DecodedAudio {
        samples: mono_samples,
        sample_rate,
        duration_secs,
    })
}

/// Linear-interpolation resampler, used to bring a track to the playback
/// engine's sample rate when two tracks in a mix differ (rare in practice —
/// nearly everything is 44.1 kHz).
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let new_len = ((samples.len() as f64) / ratio) as usize;
    (0..new_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let idx = src.floor() as usize;
            let frac = (src - idx as f64) as f32;
            let s0 = samples.get(idx).copied().unwrap_or(0.0);
            let s1 = samples.get(idx + 1).copied().unwrap_or(s0);
            s0 + (s1 - s0) * frac
        })
        .collect()
}
