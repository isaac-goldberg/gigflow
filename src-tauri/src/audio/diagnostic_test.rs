#[cfg(test)]
mod diagnostic {
    use crate::audio;
    use std::path::Path;

    #[test]
    #[ignore]
    fn dump_library_analysis() {
        let dir = std::env::var("GIGFLOW_TEST_LIBRARY").expect("set GIGFLOW_TEST_LIBRARY");
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()),
                    Some(ref ext) if ["mp3", "wav", "flac", "m4a", "aac", "ogg"].contains(&ext.as_str())
                )
            })
            .collect();
        entries.sort();

        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let decoded = audio::decode::decode_file(Path::new(&path));
            let raw_rms = decoded
                .as_ref()
                .map(|d| {
                    let sum_sq: f64 = d.samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
                    (sum_sq / d.samples.len() as f64).sqrt()
                })
                .unwrap_or(-1.0);
            let components = decoded.as_ref().ok().map(|d| {
                let bpm = audio::bpm::detect_bpm(&d.samples, d.sample_rate).bpm;
                audio::energy::compute_energy_components(&d.samples, d.sample_rate, bpm)
            });
            match audio::analyze_file(Path::new(&path)) {
                Ok(r) => {
                    let c = components.unwrap();
                    println!(
                        "{:60} bpm={:7.2} key={:4} ({:12} conf={:.2}) energy={:.3}  [tempo={:.2} onset={:.2}({:.2}/s) bass={:.2}({:.2}) bright={:.2}({:.0}Hz) loud={:.2}({:.1}dB)]  raw_rms={:.4}  dur={:.0}s",
                        name,
                        r.bpm,
                        r.key_camelot,
                        r.key_name,
                        r.key_confidence,
                        r.energy,
                        c.tempo_score,
                        c.onset_score,
                        c.onset_rate_per_sec,
                        c.bass_score,
                        c.bass_ratio,
                        c.brightness_score,
                        c.centroid_hz,
                        c.loudness_score,
                        c.loudness_db,
                        raw_rms,
                        r.duration_secs
                    )
                }
                Err(e) => println!("{:60} ERROR: {}", name, e),
            }
        }
    }

    fn clock(secs: f64) -> String {
        let m = (secs / 60.0).floor() as i64;
        let s = secs - (m as f64) * 60.0;
        format!("{}:{:05.2}", m, s)
    }

    #[test]
    #[ignore]
    fn dump_hot_cues() {
        let dir = std::env::var("GIGFLOW_TEST_LIBRARY").expect("set GIGFLOW_TEST_LIBRARY");
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()),
                    Some(ref ext) if ["mp3", "wav", "flac", "m4a", "aac", "ogg"].contains(&ext.as_str())
                )
            })
            .collect();
        entries.sort();

        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            match audio::decode::decode_file(Path::new(&path)) {
                Ok(d) => {
                    let bpm_result = audio::bpm::detect_bpm(&d.samples, d.sample_rate);
                    let cues = audio::hotcues::detect_hot_cues(&d.samples, d.sample_rate, bpm_result.bpm);
                    let beat_period = if bpm_result.bpm > 0.0 { 60.0 / bpm_result.bpm } else { 0.0 };
                    println!(
                        "{} bpm={:.2} (confidence {:.2}) beat_period={:.3}s dur={:.0}s",
                        name, bpm_result.bpm, bpm_result.confidence, beat_period, d.duration_secs
                    );
                    for c in &cues {
                        // Residual vs the *naive* (phase=0) grid, to make the
                        // phase correction's effect visible: a large residual
                        // here (often a sizeable fraction of a beat) is
                        // exactly the error the old implementation always had,
                        // on every track, by construction.
                        let naive_residual = if beat_period > 0.0 {
                            let k = (c.position_secs / beat_period).round();
                            (c.position_secs - k * beat_period).abs()
                        } else {
                            0.0
                        };
                        println!(
                            "    {:10} {:>8}  (naive-grid residual {:.3}s / {:.0}% of a beat)",
                            c.label,
                            clock(c.position_secs),
                            naive_residual,
                            if beat_period > 0.0 { naive_residual / beat_period * 100.0 } else { 0.0 }
                        );
                    }
                }
                Err(e) => println!("{:60} ERROR: {}", name, e),
            }
        }
    }
}
