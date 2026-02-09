use anyhow::Result;
#[cfg(not(target_arch = "wasm32"))]
use chrono::Utc;
use std::fs;
use std::path::Path;

pub fn normalize_heading_key(input: &str) -> String {
    let mut cleaned = input.trim().trim_end_matches(':').to_string();
    cleaned = cleaned.replace('-', " ");
    cleaned = cleaned.replace('/', " ");
    cleaned = cleaned.replace('&', " ");
    let mut out = String::new();
    let mut last_space = false;
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            last_space = false;
        } else if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        }
    }
    out.trim().to_string()
}

#[cfg(target_arch = "wasm32")]
pub fn now_iso() -> String {
    js_sys::Date::new_0().to_iso_string().into()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn read_to_string(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

pub fn write_string(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output")
        .to_string()
}
