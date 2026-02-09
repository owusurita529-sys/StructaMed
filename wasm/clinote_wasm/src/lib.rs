use serde::Serialize;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn ping() -> String {
    "pong".to_string()
}

#[wasm_bindgen]
pub fn default_format() -> String {
    format!("{:?}", clinote::models::NoteFormat::Soap).to_lowercase()
}

#[wasm_bindgen]
pub fn convert(
    note_text: String,
    template: String,
    output: String,
    strict: bool,
) -> Result<String, JsValue> {
    let template_spec = parse_template(&template)?;
    let output_format = parse_output_format(&output)?;
    let config = clinote::config::Config::default();
    let mut notes = parse_notes(&note_text, template_spec.note_format, &config);
    let inferred_flags = apply_web_soap_inference_if_needed(
        &note_text,
        &mut notes,
        template_spec,
        strict,
        &config,
    );
    let validation = build_validation_payload(&notes, template_spec, strict, &inferred_flags);
    if strict && !validation.ok {
        let strict_errors = strict_error_lines(&validation);
        return Err(JsValue::from_str(&format!(
            "Strict validation failed:\n{}",
            strict_errors.join("\n")
        )));
    }

    clinote::render::render_notes(&notes, output_format, config.csv.layout)
        .map_err(|err| JsValue::from_str(&format!("Conversion failed: {}", err)))
}

#[wasm_bindgen]
pub fn validate(note_text: String, template: String, strict: bool) -> Result<JsValue, JsValue> {
    let template_key = normalize_template_key(&template);
    let template_spec = match parse_template_spec(&template) {
        Some(spec) => spec,
        None => {
            let payload = ValidationPayload {
                ok: false,
                template: template_key,
                notes: vec![ValidationNotePayload {
                    note_index: 1,
                    errors: vec![
                        "Unsupported template. Use one of: soap, hp, discharge.".to_string(),
                    ],
                    warnings: Vec::new(),
                    info: Vec::new(),
                }],
            };
            return payload_to_js_value(&payload);
        }
    };

    let config = clinote::config::Config::default();
    let mut notes = parse_notes(&note_text, template_spec.note_format, &config);
    let inferred_flags = apply_web_soap_inference_if_needed(
        &note_text,
        &mut notes,
        template_spec,
        strict,
        &config,
    );
    let payload = build_validation_payload(&notes, template_spec, strict, &inferred_flags);
    payload_to_js_value(&payload)
}

#[wasm_bindgen]
pub fn normalize(note_text: &str, template: &str) -> Result<String, JsValue> {
    let result = normalize_internal(note_text, template)?;
    Ok(result.normalized)
}

#[wasm_bindgen]
pub fn normalize_with_stats(note_text: String, template: String) -> Result<JsValue, JsValue> {
    let result = normalize_internal(&note_text, &template)?;
    let raw = serde_json::to_string(&result)
        .map_err(|err| JsValue::from_str(&format!("Internal error: {}", err)))?;
    js_sys::JSON::parse(&raw)
        .map_err(|_| JsValue::from_str("Internal error: failed to serialize normalize payload"))
}

fn normalize_internal(note_text: &str, template: &str) -> Result<NormalizeOutput, JsValue> {
    let template_spec = parse_template(template)?;
    let config = clinote::config::Config::default();
    let mut notes = parse_notes(note_text, template_spec.note_format, &config);
    if template_spec.validation_template == clinote::validate::Template::Soap {
        let _ = apply_web_soap_inference_if_needed(
            note_text,
            &mut notes,
            template_spec,
            true,
            &config,
        );
    }

    let mut stats = NormalizeStats::default();
    let mut rendered = Vec::new();
    for (idx, note) in notes.iter().enumerate() {
        rendered.push(render_normalized_note(note, template_spec, &mut stats));
        if idx + 1 < notes.len() {
            rendered.push("---".to_string());
        }
    }
    Ok(NormalizeOutput {
        normalized: rendered.join("\n\n"),
        removed_empty_sections: stats.removed_empty_sections,
        merged_duplicates: stats.merged_duplicates,
        extracted_subjective_lines: stats.extracted_subjective_lines,
        extracted_objective_lines: stats.extracted_objective_lines,
    })
}

#[wasm_bindgen]
pub fn preview_sections(note_text: String, template: String) -> Result<JsValue, JsValue> {
    let template_spec = parse_template(&template)?;
    let config = clinote::config::Config::default();
    let notes = parse_notes(&note_text, template_spec.note_format, &config);

    let mut out = Vec::new();
    for (idx, note) in notes.iter().enumerate() {
        out.push(format!("Note {}:", idx + 1));
        for summary in clinote::validate::summarize_sections(note) {
            out.push(format!(
                "- {}: {} lines, {} chars",
                summary.name, summary.line_count, summary.char_count
            ));
        }
        if idx + 1 < notes.len() {
            out.push(String::new());
        }
    }

    Ok(JsValue::from_str(&out.join("\n")))
}

fn parse_notes(
    note_text: &str,
    note_format: clinote::models::NoteFormat,
    config: &clinote::config::Config,
) -> Vec<clinote::models::StructuredNote> {
    clinote::parser::parse_notes(
        note_text,
        note_format,
        config,
        None,
        0,
        clinote::parser::ParseOptions {
            apply_heuristics: config.enable_fallback_heuristics,
        },
    )
}

fn payload_to_js_value(payload: &ValidationPayload) -> Result<JsValue, JsValue> {
    let raw = serde_json::to_string(payload)
        .map_err(|err| JsValue::from_str(&format!("Internal error: {}", err)))?;
    js_sys::JSON::parse(&raw)
        .map_err(|_| JsValue::from_str("Internal error: failed to serialize validation payload"))
}

fn parse_template(template: &str) -> Result<TemplateSpec, JsValue> {
    parse_template_spec(template).ok_or_else(|| {
        JsValue::from_str("Unsupported template. Use one of: soap, hp, discharge.")
    })
}

fn parse_template_spec(template: &str) -> Option<TemplateSpec> {
    match normalize_template_key(template).as_str() {
        "soap" => Some(TemplateSpec {
            key: "soap",
            note_format: clinote::models::NoteFormat::Soap,
            validation_template: clinote::validate::Template::Soap,
        }),
        "hp" | "h&p" => Some(TemplateSpec {
            key: "hp",
            note_format: clinote::models::NoteFormat::Hp,
            validation_template: clinote::validate::Template::Hp,
        }),
        "discharge" | "discharge-summary" | "discharge summary" => Some(TemplateSpec {
            key: "discharge",
            note_format: clinote::models::NoteFormat::Discharge,
            validation_template: clinote::validate::Template::Discharge,
        }),
        _ => None,
    }
}

fn normalize_template_key(template: &str) -> String {
    template.trim().to_ascii_lowercase()
}

fn parse_output_format(output: &str) -> Result<clinote::render::OutputFormat, JsValue> {
    match output.trim().to_ascii_lowercase().as_str() {
        "markdown" | "md" => Ok(clinote::render::OutputFormat::Md),
        "json" => Ok(clinote::render::OutputFormat::Json),
        "csv" => Ok(clinote::render::OutputFormat::Csv),
        _ => Err(JsValue::from_str(
            "Unsupported output format. Use one of: markdown, json, csv.",
        )),
    }
}

const DUPLICATE_MIN_CONTENT_LEN: usize = 5;
const SHORT_SECTION_LEN: usize = 20;
const EMPTY_SECTION_PLACEHOLDER: &str = "No content extracted from source note.";

fn render_normalized_note(
    note: &clinote::models::StructuredNote,
    template_spec: TemplateSpec,
    stats: &mut NormalizeStats,
) -> String {
    let sections = normalize_sections(note, template_spec, stats);
    let mut out = Vec::new();
    for section in sections {
        out.push(format!("{}:", section.name));
        out.push(section.content);
        out.push(String::new());
    }
    out.join("\n").trim_end().to_string()
}

fn normalize_sections(
    note: &clinote::models::StructuredNote,
    template_spec: TemplateSpec,
    stats: &mut NormalizeStats,
) -> Vec<NormalizedSection> {
    if template_spec.validation_template == clinote::validate::Template::Soap {
        return normalize_soap_sections(note, stats);
    }

    merge_duplicate_sections(note, template_spec, stats)
}

fn merge_duplicate_sections(
    note: &clinote::models::StructuredNote,
    template_spec: TemplateSpec,
    stats: &mut NormalizeStats,
) -> Vec<NormalizedSection> {
    let mut buckets: Vec<NormalizedSection> = Vec::new();
    let mut key_index: HashMap<String, usize> = HashMap::new();

    for section in &note.sections {
        let key = normalized_section_key(&section.name);
        let content = section.content.trim();
        if content.is_empty() {
            stats.removed_empty_sections += 1;
            continue;
        }
        if let Some(idx) = key_index.get(&key).copied() {
            if content.len() < DUPLICATE_MIN_CONTENT_LEN {
                stats.removed_empty_sections += 1;
                continue;
            }
            stats.merged_duplicates += 1;
            if !buckets[idx].content.is_empty() {
                buckets[idx].content.push('\n');
            }
            buckets[idx].content.push_str(content);
            continue;
        }

        key_index.insert(key.clone(), buckets.len());
        buckets.push(NormalizedSection {
            key,
            name: canonical_section_name(&section.name, template_spec.validation_template),
            content: content.to_string(),
        });
    }

    reorder_sections(buckets, template_spec.validation_template)
}

fn reorder_sections(
    sections: Vec<NormalizedSection>,
    template: clinote::validate::Template,
) -> Vec<NormalizedSection> {
    if template != clinote::validate::Template::Soap {
        return sections;
    }

    let order = ["subjective", "objective", "assessment", "plan", "narrative"];
    let mut ordered = Vec::new();
    let mut leftovers = Vec::new();

    for section in sections {
        if let Some(order_idx) = order.iter().position(|k| *k == section.key) {
            ordered.push((order_idx, section));
        } else {
            leftovers.push(section);
        }
    }

    ordered.sort_by_key(|(idx, _)| *idx);
    let mut out = ordered.into_iter().map(|(_, s)| s).collect::<Vec<_>>();
    out.extend(leftovers);
    out
}

fn normalize_soap_sections(
    note: &clinote::models::StructuredNote,
    stats: &mut NormalizeStats,
) -> Vec<NormalizedSection> {
    let mut buckets: HashMap<String, String> = HashMap::from([
        ("subjective".to_string(), String::new()),
        ("objective".to_string(), String::new()),
        ("assessment".to_string(), String::new()),
        ("plan".to_string(), String::new()),
        ("narrative".to_string(), String::new()),
    ]);

    for section in &note.sections {
        let key = normalized_section_key(&section.name);
        let content = section.content.trim();
        if content.is_empty() {
            stats.removed_empty_sections += 1;
            continue;
        }

        let target_key = match key.as_str() {
            "subjective" | "objective" | "assessment" | "plan" | "narrative" => key.as_str(),
            _ => "narrative",
        };

        if let Some(existing) = buckets.get_mut(target_key) {
            if !existing.trim().is_empty() {
                stats.merged_duplicates += 1;
                existing.push('\n');
            }
            if target_key == "narrative" && key.as_str() != "narrative" {
                existing.push_str(&format!("{}: {}", section.name.trim(), content));
            } else {
                existing.push_str(content);
            }
        }
    }

    let narrative_original = buckets
        .get("narrative")
        .cloned()
        .unwrap_or_default();
    let mut narrative_lines = Vec::new();
    let mut extracted_assessment = Vec::new();
    let mut extracted_plan = Vec::new();

    for raw_line in narrative_original.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(text) = strip_assessment_prefix(trimmed) {
            extracted_assessment.push(text.to_string());
            continue;
        }
        if let Some(text) = strip_plan_prefix(trimmed) {
            extracted_plan.push(text.to_string());
            continue;
        }
        narrative_lines.push(trimmed.to_string());
    }

    if !extracted_assessment.is_empty() {
        append_bucket(&mut buckets, "assessment", &extracted_assessment.join("\n"), stats);
    }
    if !extracted_plan.is_empty() {
        append_bucket(&mut buckets, "plan", &extracted_plan.join("\n"), stats);
    }

    let mut remaining_lines = narrative_lines;
    let needs_subjective_inference = buckets
        .get("subjective")
        .map(|s| s.trim().len() < SHORT_SECTION_LEN)
        .unwrap_or(true);
    if needs_subjective_inference {
        let mut kept = Vec::new();
        let mut moved = Vec::new();
        for line in remaining_lines {
            if looks_subjective(&line.to_ascii_lowercase()) {
                moved.push(line);
            } else {
                kept.push(line);
            }
        }
        if !moved.is_empty() {
            stats.extracted_subjective_lines += moved.len();
            append_bucket(&mut buckets, "subjective", &moved.join("\n"), stats);
        }
        remaining_lines = kept;
    }

    let needs_objective_inference = buckets
        .get("objective")
        .map(|s| s.trim().len() < SHORT_SECTION_LEN)
        .unwrap_or(true);
    if needs_objective_inference {
        let mut kept = Vec::new();
        let mut moved = Vec::new();
        for line in remaining_lines {
            if looks_objective(&line.to_ascii_lowercase()) {
                moved.push(line);
            } else {
                kept.push(line);
            }
        }
        if !moved.is_empty() {
            stats.extracted_objective_lines += moved.len();
            append_bucket(&mut buckets, "objective", &moved.join("\n"), stats);
        }
        remaining_lines = kept;
    }

    buckets.insert("narrative".to_string(), remaining_lines.join("\n"));

    let ordered_keys = [
        ("subjective", "Subjective"),
        ("objective", "Objective"),
        ("assessment", "Assessment"),
        ("plan", "Plan"),
        ("narrative", "Narrative"),
    ];

    ordered_keys
        .iter()
        .map(|(key, name)| {
            let content = buckets.get(*key).cloned().unwrap_or_default();
            NormalizedSection {
                key: (*key).to_string(),
                name: (*name).to_string(),
                content: if content.trim().is_empty() {
                    EMPTY_SECTION_PLACEHOLDER.to_string()
                } else {
                    content.trim().to_string()
                },
            }
        })
        .collect()
}

fn append_bucket(
    buckets: &mut HashMap<String, String>,
    key: &str,
    content: &str,
    stats: &mut NormalizeStats,
) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    let entry = buckets.entry(key.to_string()).or_default();
    if !entry.trim().is_empty() {
        stats.merged_duplicates += 1;
        entry.push('\n');
    }
    entry.push_str(trimmed);
}

fn normalized_section_key(name: &str) -> String {
    let key = name.trim().to_ascii_lowercase();
    match key.as_str() {
        "s" => "subjective".to_string(),
        "o" => "objective".to_string(),
        "a" | "dx" | "diagnosis" => "assessment".to_string(),
        "p" => "plan".to_string(),
        _ => key,
    }
}

fn canonical_section_name(raw_name: &str, template: clinote::validate::Template) -> String {
    let key = normalized_section_key(raw_name);
    if template == clinote::validate::Template::Soap {
        return match key.as_str() {
            "subjective" => "Subjective".to_string(),
            "objective" => "Objective".to_string(),
            "assessment" => "Assessment".to_string(),
            "plan" => "Plan".to_string(),
            "narrative" => "Narrative".to_string(),
            _ => raw_name.trim().to_string(),
        };
    }
    raw_name.trim().to_string()
}

fn strip_assessment_prefix(line: &str) -> Option<&str> {
    strip_known_prefix(line, &["assessment:", "dx:", "a:", "a "])
}

fn strip_plan_prefix(line: &str) -> Option<&str> {
    strip_known_prefix(line, &["plan:", "tx:", "p:", "plan ", "p "])
}

fn strip_known_prefix<'a>(line: &'a str, prefixes: &[&str]) -> Option<&'a str> {
    let lower = line.to_ascii_lowercase();
    for prefix in prefixes {
        if lower.starts_with(prefix) {
            let stripped = line[prefix.len()..].trim();
            if !stripped.is_empty() {
                return Some(stripped);
            }
            return None;
        }
    }
    None
}

fn build_validation_payload(
    notes: &[clinote::models::StructuredNote],
    template_spec: TemplateSpec,
    strict: bool,
    inferred_flags: &[bool],
) -> ValidationPayload {
    let mut note_payloads = Vec::new();
    let mut ok = true;

    for (idx, note) in notes.iter().enumerate() {
        let issues = clinote::validate::validate_note(note, template_spec.validation_template, strict);
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut info = Vec::new();

        for issue in issues {
            match issue.severity {
                clinote::validate::Severity::Error => errors.push(issue.message),
                clinote::validate::Severity::Warn => warnings.push(issue.message),
                clinote::validate::Severity::Info => info.push(issue.message),
            }
        }

        if inferred_flags.get(idx).copied().unwrap_or(false) {
            info.push("Inferred Subjective/Objective from unstructured text (web-only).".to_string());
        }

        if !errors.is_empty() {
            ok = false;
        }

        note_payloads.push(ValidationNotePayload {
            note_index: note.note_index,
            errors,
            warnings,
            info,
        });
    }

    ValidationPayload {
        ok,
        template: template_spec.key.to_string(),
        notes: note_payloads,
    }
}

fn strict_error_lines(payload: &ValidationPayload) -> Vec<String> {
    let mut lines = Vec::new();
    for note in &payload.notes {
        for err in &note.errors {
            lines.push(format!("Note {}: {}", note.note_index, err));
        }
    }
    if lines.is_empty() {
        lines.push("No strict-validation error details were produced.".to_string());
    }
    lines
}

fn apply_web_soap_inference_if_needed(
    note_text: &str,
    notes: &mut [clinote::models::StructuredNote],
    template_spec: TemplateSpec,
    strict: bool,
    config: &clinote::config::Config,
) -> Vec<bool> {
    let mut inferred_flags = vec![false; notes.len()];
    if !(strict && template_spec.validation_template == clinote::validate::Template::Soap) {
        return inferred_flags;
    }

    let (split_notes, _) = clinote::parser::split_bundle(note_text, config.bundle.mode_default, config);

    for (idx, note) in notes.iter_mut().enumerate() {
        let issues = clinote::validate::validate_note(note, clinote::validate::Template::Soap, true);
        let missing_subjective = has_missing_required_section(&issues, "Subjective");
        let missing_objective = has_missing_required_section(&issues, "Objective");
        if !missing_subjective && !missing_objective {
            continue;
        }

        let raw_note = split_notes.get(idx).map(String::as_str).unwrap_or(note_text);
        let (subjective_lines, objective_lines) = infer_soap_buckets(raw_note);
        let mut inferred_sections = Vec::new();

        if missing_subjective && !subjective_lines.is_empty() {
            inferred_sections.push(clinote::models::Section {
                name: "Subjective".to_string(),
                content: subjective_lines.join("\n"),
                confidence: 0.55,
            });
        }
        if missing_objective && !objective_lines.is_empty() {
            inferred_sections.push(clinote::models::Section {
                name: "Objective".to_string(),
                content: objective_lines.join("\n"),
                confidence: 0.55,
            });
        }

        if !inferred_sections.is_empty() {
            let mut merged = inferred_sections;
            merged.extend(note.sections.clone());
            note.sections = merged;
            inferred_flags[idx] = true;
        }
    }

    inferred_flags
}

fn has_missing_required_section(issues: &[clinote::validate::ValidationIssue], section_name: &str) -> bool {
    issues.iter().any(|issue| {
        issue.severity == clinote::validate::Severity::Error
            && issue.code == "missing_required"
            && issue
                .section
                .as_deref()
                .map(|section| section.eq_ignore_ascii_case(section_name))
                .unwrap_or(false)
    })
}

fn infer_soap_buckets(raw_note: &str) -> (Vec<String>, Vec<String>) {
    let mut subjective = Vec::new();
    let mut objective = Vec::new();

    for line in raw_note.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();

        if looks_subjective(&lower) {
            subjective.push(trimmed.to_string());
            continue;
        }

        if looks_objective(&lower) {
            objective.push(trimmed.to_string());
        }
    }

    (subjective, objective)
}

fn looks_subjective(line: &str) -> bool {
    contains_any(
        line,
        &[
            "cc",
            "chief complaint",
            "hpi",
            "pmh",
            "history",
            "meds",
            "medication",
            "allerg",
            "subjective",
        ],
    )
}

fn looks_objective(line: &str) -> bool {
    contains_any(
        line,
        &[
            "vitals",
            "bp",
            "hr",
            "temp",
            "spo2",
            "o2 sat",
            "exam",
            "physical",
            "objective",
            "neuro",
        ],
    )
}

fn contains_any(line: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| line.contains(marker))
}

#[derive(Clone, Copy)]
struct TemplateSpec {
    key: &'static str,
    note_format: clinote::models::NoteFormat,
    validation_template: clinote::validate::Template,
}

struct NormalizedSection {
    key: String,
    name: String,
    content: String,
}

#[derive(Default)]
struct NormalizeStats {
    removed_empty_sections: usize,
    merged_duplicates: usize,
    extracted_subjective_lines: usize,
    extracted_objective_lines: usize,
}

#[derive(Serialize)]
struct NormalizeOutput {
    normalized: String,
    removed_empty_sections: usize,
    merged_duplicates: usize,
    extracted_subjective_lines: usize,
    extracted_objective_lines: usize,
}

#[derive(Serialize)]
struct ValidationPayload {
    ok: bool,
    template: String,
    notes: Vec<ValidationNotePayload>,
}

#[derive(Serialize)]
struct ValidationNotePayload {
    note_index: usize,
    errors: Vec<String>,
    warnings: Vec<String>,
    info: Vec<String>,
}
