use clinote::config::Config;
use clinote::models::{Metadata, NoteFormat, Section, StructuredNote};
use clinote::parser;
use clinote::render::{self, OutputFormat};
use clinote::validate::{self, Severity, Template};
use std::fs;

fn fixture(path: &str) -> String {
    fs::read_to_string(path).expect("fixture should load")
}

fn make_note(format: NoteFormat, sections: Vec<(&str, &str)>) -> StructuredNote {
    StructuredNote {
        id: "test-note".to_string(),
        format,
        source_file: None,
        note_index: 1,
        sections: sections
            .into_iter()
            .map(|(name, content)| Section {
                name: name.to_string(),
                content: content.to_string(),
                confidence: 0.9,
            })
            .collect(),
        warnings: Vec::new(),
        metadata: Metadata {
            generated_at: "2024-01-01T00:00:00Z".to_string(),
            tool_version: "0.1.0".to_string(),
        },
    }
}

#[test]
fn validate_strict_fails_when_missing_required_section() {
    let note = make_note(NoteFormat::Soap, vec![("Subjective", "short")]);
    let issues = validate::validate_note(&note, Template::Soap, true);
    assert!(issues.iter().any(|i| i.severity == Severity::Error));
}

#[test]
fn validate_non_strict_warns_instead_of_fails() {
    let note = make_note(NoteFormat::Soap, vec![("Subjective", "short")]);
    let issues = validate::validate_note(&note, Template::Soap, false);
    assert!(!issues.iter().any(|i| i.severity == Severity::Error));
    assert!(issues.iter().any(|i| i.severity == Severity::Warn));
}

#[test]
fn preview_lists_sections() {
    let config = Config::default();
    let input = fixture("tests/fixtures/soap_messy.txt");
    let (candidates, _warnings) = parser::extract_candidates(
        &input,
        NoteFormat::Soap,
        &config,
        parser::ParseOptions {
            apply_heuristics: config.enable_fallback_heuristics,
        },
    );
    let note = parser::build_note(candidates, NoteFormat::Soap, None, 1, Vec::new());
    let summary = validate::summarize_sections(&note);
    assert!(summary.iter().any(|s| s.name == "Subjective"));
    assert!(summary.iter().all(|s| s.line_count > 0));
}

#[test]
fn convert_output_matches_baseline() {
    let config = Config::default();
    let input = fixture("tests/fixtures/soap_messy.txt");
    let (candidates, _warnings) = parser::extract_candidates(
        &input,
        NoteFormat::Soap,
        &config,
        parser::ParseOptions {
            apply_heuristics: config.enable_fallback_heuristics,
        },
    );
    let note = parser::build_note(
        candidates,
        NoteFormat::Soap,
        Some("tests/fixtures/soap_messy.txt".to_string()),
        1,
        Vec::new(),
    );
    let output = render::render_notes(&[note], OutputFormat::Md, config.csv.layout).unwrap();
    let expected = fixture("tests/fixtures/soap_messy.expected.md");
    assert_eq!(output.trim_end(), expected.trim_end());
}
