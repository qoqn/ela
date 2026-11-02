use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::CommandResult;

const FILE_NAME: &str = "benchmarks.jsonl";

#[tauri::command]
#[specta::specta]
pub fn record_benchmark(app: tauri::AppHandle, sample: Sample) -> CommandResult<()> {
    use tauri::Manager;
    let dir = app.path().app_log_dir()?;
    Ok(append(&dir, &sample)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Sample {
    pub run: String,
    pub name: String,
    pub renderer: String,
    pub bytes_per_sec: u64,
    pub paints_per_sec: u64,
    pub produced_per_sec: Option<u64>,
    pub sent_per_sec: Option<u64>,
    #[serde(default)]
    pub stages_ms: BTreeMap<String, f64>,
}

#[derive(Serialize)]
struct Line<'a> {
    unix_ms: u128,
    #[serde(flatten)]
    sample: &'a Sample,
}

fn append(dir: &Path, sample: &Sample) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut line = serde_json::to_vec(&Line { unix_ms, sample })?;
    line.push(b'\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE_NAME))?
        .write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_one_json_line_per_sample() {
        let dir = std::env::temp_dir().join(format!("ela-benchmark-log-{}", uuid::Uuid::new_v4()));
        let sample = Sample {
            run: "r1".into(),
            name: "Full HD, every pixel".into(),
            renderer: "framebuffer".into(),
            bytes_per_sec: 276,
            paints_per_sec: 36,
            produced_per_sec: Some(60),
            sent_per_sec: None,
            stages_ms: BTreeMap::from([("put".into(), 12.5)]),
        };
        append(&dir, &sample).unwrap();
        append(&dir, &sample).unwrap();

        let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["name"], "Full HD, every pixel");
        assert_eq!(lines[0]["paints_per_sec"], 36);
        assert_eq!(lines[0]["stages_ms"]["put"], 12.5);
        assert!(lines[0]["unix_ms"].as_u64().unwrap() > 0);
        fs::remove_dir_all(dir).unwrap();
    }
}
