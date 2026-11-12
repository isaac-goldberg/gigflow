use crate::genre::{LLM_EDM_SUBGENRES, LLM_GENRES};
use crate::models::AiGenreSuggestion;
use serde_json::json;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PORT: u16 = 8799;
const MODEL_FILENAME: &str = "Qwen3-4B-Q4_K_M.gguf";
const MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q4_K_M.gguf";

/// Manages the `llama-server` sidecar process that serves the local genre
/// classifier. Compared against three candidate models (Qwen3-1.7B,
/// Qwen3-4B, Llama-3.2-3B-Instruct) on real library tracks before picking
/// Qwen3-4B: it was the only one that didn't collapse to a single
/// catch-all subgenre guess across clearly different tracks, and it's
/// Apache-2.0 (no commercial-use licensing question, unlike some audio
/// genre-classification models).
///
/// Started lazily on first use, not at app launch — most sessions won't
/// touch this feature, and loading a ~2.5GB model has a real (if one-time
/// per launch) cost.
pub struct Llm(Arc<Mutex<LlmInner>>);

struct LlmInner {
    child: Option<Child>,
}

impl Drop for LlmInner {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

impl Llm {
    pub fn new() -> Self {
        Llm(Arc::new(Mutex::new(LlmInner { child: None })))
    }
}

impl Clone for Llm {
    fn clone(&self) -> Self {
        Llm(self.0.clone())
    }
}

pub fn model_path() -> PathBuf {
    crate::db::app_data_dir().join("models").join(MODEL_FILENAME)
}

pub fn model_downloaded() -> bool {
    model_path().is_file()
}

/// Downloads the quantized GGUF weights for the bundled genre classifier —
/// ~2.5GB, so this is only ever called from an explicit "Download AI
/// model" action in the UI, never automatically.
pub fn download_model() -> Result<(), String> {
    let dest = model_path();
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = dest.with_extension("gguf.partial");
    let resp = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut file).map_err(|e| format!("download failed: {e}"))?;
    drop(file);
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

pub struct TrackInfo<'a> {
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub bpm: Option<f64>,
    pub key_camelot: Option<&'a str>,
    pub energy: Option<f64>,
}

/// Starts (or confirms already-running) the llama-server sidecar, blocking
/// the calling thread until it's healthy or it times out — callers must run
/// this off the async runtime (see `commands::classify_genre`), since a
/// cold model load can take several seconds.
fn ensure_server(llm: &Llm) -> Result<u16, String> {
    let mut inner = llm.0.lock().unwrap();
    if let Some(child) = inner.child.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(PORT); // still running
        }
        inner.child = None; // exited since last check; respawn below
    }

    let path = model_path();
    if !path.is_file() {
        return Err(
            "the local AI genre model hasn't been downloaded yet — use \"Download AI genre model\" first"
                .to_string(),
        );
    }

    let child = Command::new("llama-server")
        .args([
            "-m",
            path.to_str().unwrap(),
            "--port",
            &PORT.to_string(),
            "-c",
            "4096",
            "--no-warmup",
            "-ngl",
            "99",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "couldn't start llama-server (is llama.cpp installed? on macOS: `brew install llama.cpp`): {e}"
            )
        })?;
    inner.child = Some(child);
    drop(inner);

    let start = Instant::now();
    loop {
        if ureq::get(&format!("http://127.0.0.1:{PORT}/health"))
            .timeout(Duration::from_secs(2))
            .call()
            .is_ok()
        {
            return Ok(PORT);
        }
        if start.elapsed() > Duration::from_secs(60) {
            return Err("timed out waiting for the local AI model to start".to_string());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn system_prompt() -> String {
    format!(
        "You are a music genre classification assistant for a DJ library app. Given a track's \
         title, artist, tempo (BPM), musical key (Camelot notation) and energy (0-1, higher = more \
         intense), classify it into exactly one genre from this fixed list: {genres}. If (and only \
         if) the genre is EDM, also pick exactly one subgenre from this fixed list: {subgenres}. \
         For any other genre, subgenre must be null.\n\n\
         Title and artist are your strongest signal — use what you actually know about the real \
         artist, label or scene. BPM/key/energy are weak supporting signals only.\n\n\
         IMPORTANT — don't default to the plain \"House\" subgenre as a generic fallback for any \
         uptempo EDM track you're not sure about. House, Progressive House, Big Room House, \
         Electro, Bass House, Tech House and Afro House all commonly share the same ~118-128 BPM \
         range, so BPM alone cannot tell them apart. Pick the MOST SPECIFIC subgenre your \
         knowledge of the actual artist/scene supports (e.g. a well-known big-room/festival \
         artist is \"Big Room House\" or \"Progressive House\", not plain \"House\"; a \
         bass-music/dubstep-adjacent artist who blends in four-on-the-floor elements is \"Bass \
         House\", not \"Dubstep\" or plain \"House\"). Only choose plain \"House\" when you \
         specifically recognize a deep/classic/tech-house style, never as a default.\n\n\
         If you don't recognize the artist/track and the other signals aren't diagnostic, don't \
         force a specific subgenre guess — set subgenre to the closest you can genuinely justify \
         (or null if nothing fits better than a coin flip) and set confidence to \"low\" rather \
         than refusing or guessing confidently. Respond with JSON only.",
        genres = LLM_GENRES.join(", "),
        subgenres = LLM_EDM_SUBGENRES.join(", "),
    )
}

fn response_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "genre": { "type": "string", "enum": LLM_GENRES },
            "subgenre": { "anyOf": [{ "type": "string", "enum": LLM_EDM_SUBGENRES }, { "type": "null" }] },
            "confidence": { "type": "string", "enum": ["high", "medium", "low"] }
        },
        "required": ["genre", "subgenre", "confidence"],
        "additionalProperties": false
    })
}

/// Classifies one track, starting the sidecar if it isn't already running.
/// Blocking — run off the async runtime.
pub fn classify(llm: &Llm, track: &TrackInfo) -> Result<AiGenreSuggestion, String> {
    let port = ensure_server(llm)?;

    let user_content = format!(
        "Title: {}\nArtist: {}\nBPM: {}\nKey (Camelot): {}\nEnergy (0-1): {}\n\nClassify this track. /no_think",
        track.title,
        track.artist.unwrap_or("Unknown"),
        track.bpm.map(|b| b.to_string()).unwrap_or_else(|| "Unknown".to_string()),
        track.key_camelot.unwrap_or("Unknown"),
        track.energy.map(|e| e.to_string()).unwrap_or_else(|| "Unknown".to_string()),
    );

    let payload = json!({
        "messages": [
            { "role": "system", "content": system_prompt() },
            { "role": "user", "content": user_content },
        ],
        "temperature": 0.1,
        "max_tokens": 200,
        "response_format": {
            "type": "json_schema",
            "json_schema": { "name": "genre_classification", "schema": response_schema(), "strict": true }
        },
    });

    let resp: serde_json::Value = ureq::post(&format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .timeout(Duration::from_secs(60))
        .send_json(payload)
        .map_err(|e| format!("the local AI model request failed: {e}"))?
        .into_json()
        .map_err(|e| format!("the local AI model returned an unreadable response: {e}"))?;

    let mut content = resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("the local AI model's response had no content")?
        .to_string();

    // Qwen3 sometimes emits a <think>...</think> block even with /no_think —
    // strip it defensively rather than trust the hint always works.
    if let Some(idx) = content.find("</think>") {
        content = content[idx + "</think>".len()..].trim().to_string();
    }

    serde_json::from_str(&content)
        .map_err(|e| format!("couldn't parse the local AI model's output as JSON: {e} (raw: {content})"))
}
