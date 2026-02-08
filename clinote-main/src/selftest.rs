use crate::config::Config;
use crate::parser::{self, ParseOptions};
use crate::render::{self, OutputFormat};
use crate::validate::{self, Severity, Template, ValidationIssue};
use crate::util;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResult {
    pub file: String,
    pub notes: usize,
    pub errors: usize,
    pub warnings: usize,
    pub issues: Vec<ValidationIssue>,
    pub runtime_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelftestSummary {
    pub fixtures: String,
    pub template: Template,
    pub strict: bool,
    pub total_files: usize,
    pub total_notes: usize,
    pub total_errors: usize,
    pub total_warnings: usize,
    pub runtime_failures: usize,
    pub top_failing: Vec<FileResult>,
}

pub fn run_selftest(
    fixtures: &str,
    template: Template,
    strict: bool,
    out_dir: Option<&Path>,
) -> Result<SelftestSummary> {
    let config = Config::default();
    let files = collect_files(fixtures)?;
    let mut results = Vec::new();

    for path in files {
        let result = process_file(&path, template, strict, out_dir, &config);
        results.push(result);
    }

    Ok(summarize(fixtures, template, strict, results))
}

fn collect_files(fixtures: &str) -> Result<Vec<PathBuf>> {
    let path = Path::new(fixtures);
    if path.exists() && path.is_dir() {
        let mut files = Vec::new();
        visit_dir(path, &mut files)?;
        files.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"));
        files.sort();
        return Ok(files);
    }

    if has_glob_meta(fixtures) {
        let mut files = Vec::new();
        for entry in glob::glob(fixtures)? {
            match entry {
                Ok(path) => files.push(path),
                Err(_) => {}
            }
        }
        files.sort();
        return Ok(files);
    }

    if path.exists() {
        return Ok(vec![path.to_path_buf()]);
    }

    Err(anyhow!("Fixtures path not found: {}", fixtures))
}

fn has_glob_meta(input: &str) -> bool {
    input.contains('*') || input.contains('?') || input.contains('[') || input.contains('{')
}

fn visit_dir(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit_dir(&path, files)?;
        } else {
            files.push(path);
        }
    }
    Ok(())
}

fn process_file(
    path: &Path,
    template: Template,
    strict: bool,
    out_dir: Option<&Path>,
    config: &Config,
) -> FileResult {
    match util::read_to_string(path) {
        Ok(content) => {
            let (note_texts, bundle_warnings) =
                parser::split_bundle(&content, config.bundle.mode_default, config);
            let mut all_issues = Vec::new();
            let mut notes = Vec::new();

            for (idx, note_text) in note_texts.iter().enumerate() {
                let (candidates, mut warnings) = parser::extract_candidates(
                    note_text,
                    template_to_format(template),
                    config,
                    ParseOptions {
                        apply_heuristics: config.enable_fallback_heuristics,
                    },
                );
                warnings.extend(bundle_warnings.clone());
                let note = parser::build_note(
                    candidates,
                    template_to_format(template),
                    Some(path.display().to_string()),
                    idx + 1,
                    warnings,
                );
                let issues = validate::validate_note(&note, template, strict);
                all_issues.extend(issues);
                notes.push(note);
            }

            if let Some(out_dir) = out_dir {
                let stem = util::file_stem(path);
                let md = render::render_notes(&notes, OutputFormat::Md, config.csv.layout)
                    .unwrap_or_else(|_| "".to_string());
                let json = render::render_notes(&notes, OutputFormat::Json, config.csv.layout)
                    .unwrap_or_else(|_| "".to_string());
                let csv = render::render_notes(&notes, OutputFormat::Csv, config.csv.layout)
                    .unwrap_or_else(|_| "".to_string());
                let _ = util::write_string(&out_dir.join(format!("{}.md", stem)), &md);
                let _ = util::write_string(&out_dir.join(format!("{}.json", stem)), &json);
                let _ = util::write_string(&out_dir.join(format!("{}.csv", stem)), &csv);
            }

            let errors = all_issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let warnings = all_issues
                .iter()
                .filter(|i| i.severity == Severity::Warn)
                .count();

            FileResult {
                file: path.display().to_string(),
                notes: notes.len(),
                errors,
                warnings,
                issues: all_issues,
                runtime_error: None,
            }
        }
        Err(err) => FileResult {
            file: path.display().to_string(),
            notes: 0,
            errors: 0,
            warnings: 0,
            issues: Vec::new(),
            runtime_error: Some(err.to_string()),
        },
    }
}

fn summarize(fixtures: &str, template: Template, strict: bool, results: Vec<FileResult>) -> SelftestSummary {
    let mut total_files = 0;
    let mut total_notes = 0;
    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut runtime_failures = 0;

    for result in &results {
        total_files += 1;
        total_notes += result.notes;
        total_errors += result.errors;
        total_warnings += result.warnings;
        if result.runtime_error.is_some() {
            runtime_failures += 1;
        }
    }

    let mut top = results.clone();
    top.sort_by_key(|r| (std::cmp::Reverse(r.errors), std::cmp::Reverse(r.warnings)));
    top.truncate(5);

    SelftestSummary {
        fixtures: fixtures.to_string(),
        template,
        strict,
        total_files,
        total_notes,
        total_errors,
        total_warnings,
        runtime_failures,
        top_failing: top,
    }
}

fn template_to_format(template: Template) -> crate::models::NoteFormat {
    match template {
        Template::Soap => crate::models::NoteFormat::Soap,
        Template::Hp => crate::models::NoteFormat::Hp,
        Template::Discharge => crate::models::NoteFormat::Discharge,
    }
}

pub fn summarize_text(summary: &SelftestSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!("Fixtures: {}\n", summary.fixtures));
    out.push_str(&format!("Template: {:?}\n", summary.template));
    out.push_str(&format!("Strict: {}\n", summary.strict));
    out.push_str(&format!("Total files: {}\n", summary.total_files));
    out.push_str(&format!("Total notes: {}\n", summary.total_notes));
    out.push_str(&format!("Total errors: {}\n", summary.total_errors));
    out.push_str(&format!("Total warnings: {}\n", summary.total_warnings));
    out.push_str(&format!("Runtime failures: {}\n", summary.runtime_failures));
    out.push_str("Top failing files:\n");
    for result in &summary.top_failing {
        if result.errors > 0 || result.warnings > 0 || result.runtime_error.is_some() {
            let reason = result
                .runtime_error
                .clone()
                .unwrap_or_else(|| format!("{} errors, {} warnings", result.errors, result.warnings));
            out.push_str(&format!("- {}: {}\n", result.file, reason));
        }
    }
    out
}
