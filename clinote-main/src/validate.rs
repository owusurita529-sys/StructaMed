use crate::models::StructuredNote;
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub code: String,
    pub message: String,
    pub severity: Severity,
    pub section: Option<String>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Template {
    Soap,
    Hp,
    Discharge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionSummary {
    pub name: String,
    pub line_count: usize,
    pub char_count: usize,
}

const MIN_SECTION_LEN: usize = 20;

pub fn validate_note(
    note: &StructuredNote,
    template: Template,
    strict: bool,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let groups = required_groups(template);
    let known = known_sections(template);

    let mut counts: HashMap<String, usize> = HashMap::new();
    for section in &note.sections {
        let key = util::normalize_heading_key(&section.name);
        *counts.entry(key).or_insert(0) += 1;
    }

    for group in groups {
        let mut present = false;
        for alias in &group {
            let key = util::normalize_heading_key(alias);
            if counts.get(&key).copied().unwrap_or(0) > 0 {
                present = true;
                break;
            }
        }
        if !present {
            let severity = if strict {
                Severity::Error
            } else {
                Severity::Warn
            };
            issues.push(ValidationIssue {
                code: "missing_required".to_string(),
                message: format!(
                    "Missing required section ({})",
                    group.first().cloned().unwrap_or_default()
                ),
                severity,
                section: group.first().cloned(),
                span: None,
            });
        }
    }

    for section in &note.sections {
        let key = util::normalize_heading_key(&section.name);
        if counts.get(&key).copied().unwrap_or(0) > 1 {
            issues.push(ValidationIssue {
                code: "duplicate_section".to_string(),
                message: format!("Duplicate section '{}'", section.name),
                severity: Severity::Warn,
                section: Some(section.name.clone()),
                span: None,
            });
        }

        if !known.contains(&key) {
            issues.push(ValidationIssue {
                code: "unknown_section".to_string(),
                message: format!("Unknown section '{}'", section.name),
                severity: Severity::Info,
                section: Some(section.name.clone()),
                span: None,
            });
        }

        let trimmed = section.content.trim();
        if trimmed.is_empty() || trimmed.len() < MIN_SECTION_LEN {
            issues.push(ValidationIssue {
                code: "section_too_short".to_string(),
                message: format!("Section '{}' is empty or too short", section.name),
                severity: Severity::Warn,
                section: Some(section.name.clone()),
                span: None,
            });
        }
    }

    issues
}

pub fn summarize_sections(note: &StructuredNote) -> Vec<SectionSummary> {
    note.sections
        .iter()
        .map(|section| {
            let line_count = section.content.lines().count().max(1);
            SectionSummary {
                name: section.name.clone(),
                line_count,
                char_count: section.content.chars().count(),
            }
        })
        .collect()
}

fn required_groups(template: Template) -> Vec<Vec<String>> {
    match template {
        Template::Soap => vec![
            vec!["Subjective".to_string(), "S".to_string()],
            vec!["Objective".to_string(), "O".to_string()],
            vec![
                "Assessment".to_string(),
                "A".to_string(),
                "Diagnosis".to_string(),
                "Dx".to_string(),
            ],
            vec!["Plan".to_string(), "P".to_string()],
        ],
        Template::Hp => vec![
            vec!["HPI".to_string(), "History of Present Illness".to_string()],
            vec![
                "PMH".to_string(),
                "Past Medical History".to_string(),
                "Hx".to_string(),
            ],
            vec!["Medications".to_string(), "Meds".to_string()],
            vec!["Allergies".to_string(), "Allergy".to_string()],
            vec![
                "Physical Exam".to_string(),
                "Exam".to_string(),
                "PE".to_string(),
            ],
            vec![
                "Assessment".to_string(),
                "Dx".to_string(),
                "Diagnosis".to_string(),
            ],
            vec!["Plan".to_string(), "P".to_string()],
        ],
        Template::Discharge => vec![
            vec![
                "Admission Dx".to_string(),
                "Discharge Dx".to_string(),
                "Diagnoses".to_string(),
                "Diagnosis".to_string(),
            ],
            vec![
                "Hospital Course".to_string(),
                "HospitalCourse".to_string(),
                "Course".to_string(),
            ],
            vec![
                "Medications".to_string(),
                "Discharge Meds".to_string(),
                "DischargeMeds".to_string(),
            ],
            vec![
                "Follow-up".to_string(),
                "Follow Up".to_string(),
                "FollowUp".to_string(),
            ],
        ],
    }
}

fn known_sections(template: Template) -> HashSet<String> {
    let mut all = HashSet::new();
    for group in required_groups(template) {
        for name in group {
            all.insert(util::normalize_heading_key(&name));
        }
    }
    let optional = match template {
        Template::Soap => vec!["Narrative"],
        Template::Hp => vec!["Chief Complaint", "ROS", "Review of Systems", "Narrative"],
        Template::Discharge => vec!["Disposition", "Instructions", "Narrative"],
    };
    for name in optional {
        all.insert(util::normalize_heading_key(name));
    }
    all
}
