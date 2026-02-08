use crate::config::Config;
use crate::interactive;
use crate::models::{BundleMode, NoteFormat};
use crate::parser::{self, ParseOptions};
use crate::render::{self, OutputFormat};
use crate::reports::BatchReport;
use crate::samples;
use crate::selftest;
use crate::util;
use crate::validate::{self, Severity, Template, ValidationIssue};
use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use glob::glob;
use std::path::{Path, PathBuf};
use std::process;
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "clinote",
    version,
    about = "Clinote CLI: deterministic clinical note structuring",
    after_help = "Examples:\n  clinote validate notes.txt --template soap --strict\n  clinote preview notes.txt --template hp\n  clinote init --path clinote.toml\n  clinote demo\n"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Parse(ParseArgs),
    Batch(BatchArgs),
    Sample(SampleArgs),
    Validate(ValidateArgs),
    Preview(PreviewArgs),
    Init(InitArgs),
    Demo(DemoArgs),
    Selftest(SelftestArgs),
}

#[derive(Args, Debug, Clone)]
pub struct ParseArgs {
    #[arg(long)]
    pub input: PathBuf,
    #[arg(long, value_enum)]
    pub format: NoteFormat,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long, value_enum)]
    pub out_format: OutputFormat,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub bundle: Option<BundleMode>,
    #[arg(long)]
    pub interactive: bool,
}

#[derive(Args, Debug, Clone)]
pub struct BatchArgs {
    #[arg(long)]
    pub input_dir: PathBuf,
    #[arg(long)]
    pub glob: Option<String>,
    #[arg(long, value_enum)]
    pub format: NoteFormat,
    #[arg(long)]
    pub out_dir: PathBuf,
    #[arg(long, value_enum)]
    pub out_format: OutputFormat,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub bundle: Option<BundleMode>,
}

#[derive(Args, Debug, Clone)]
pub struct SampleArgs {
    #[arg(long)]
    pub out_dir: PathBuf,
    #[arg(long)]
    pub n: usize,
    #[arg(long)]
    pub bundles: Option<usize>,
}

#[derive(Args, Debug, Clone)]
#[command(
    about = "Validate a clinical note or config",
    long_about = "Validate an input note against a template or validate a config file.\nExamples:\n  clinote validate notes.txt --template soap --strict\n  clinote validate --config clinote.toml\n"
)]
pub struct ValidateArgs {
    #[arg(value_name = "INPUT")]
    pub input: Option<PathBuf>,
    #[arg(long, value_enum)]
    pub template: Option<Template>,
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
#[command(
    about = "Preview detected sections",
    long_about = "Preview detected sections and line counts.\nExample:\n  clinote preview notes.txt --template hp\n"
)]
pub struct PreviewArgs {
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,
    #[arg(long, value_enum)]
    pub template: Option<Template>,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
#[command(
    about = "Create a default config file",
    long_about = "Create a default clinote.toml configuration template."
)]
pub struct InitArgs {
    #[arg(long, default_value = "clinote.toml")]
    pub path: PathBuf,
}

#[derive(Args, Debug, Clone)]
#[command(
    about = "Run demo conversion + validation",
    long_about = "Generate sample notes, convert them, and run validation reports."
)]
pub struct DemoArgs {
    #[arg(long, default_value = "demo_outputs")]
    pub out_dir: PathBuf,
}

#[derive(Args, Debug, Clone)]
#[command(
    about = "Run selftest on fixtures",
    long_about = "Scan fixtures, validate notes, and optionally render outputs.\nExample:\n  clinote selftest --fixtures tests/fixtures --template soap --strict --json\n"
)]
pub struct SelftestArgs {
    #[arg(long)]
    pub fixtures: String,
    #[arg(long, value_enum)]
    pub template: Option<Template>,
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Parse(args) => run_parse(&args),
        Commands::Batch(args) => run_batch_command(&args),
        Commands::Sample(args) => run_sample(&args),
        Commands::Validate(args) => run_validate(&args),
        Commands::Preview(args) => run_preview(&args),
        Commands::Init(args) => run_init(&args),
        Commands::Demo(args) => run_demo(&args),
        Commands::Selftest(args) => run_selftest(&args),
    }
}

fn run_parse(args: &ParseArgs) -> Result<()> {
    let config = Config::load(args.config.as_deref())?;
    let input = util::read_to_string(&args.input)?;
    let bundle_mode = args.bundle.unwrap_or(config.bundle.mode_default);
    let (note_texts, bundle_warnings) = parser::split_bundle(&input, bundle_mode, &config);

    let apply_heuristics = if args.interactive {
        interactive::prompt_apply_heuristics()?
    } else {
        config.enable_fallback_heuristics
    };

    let mut notes = Vec::new();
    for (idx, note_text) in note_texts.iter().enumerate() {
        let (candidates, mut warnings) = parser::extract_candidates(
            note_text,
            args.format,
            &config,
            ParseOptions { apply_heuristics },
        );
        warnings.extend(bundle_warnings.clone());

        let selected = if args.interactive {
            interactive::review_sections(&candidates)?
        } else {
            candidates
        };

        let note = parser::build_note(
            selected,
            args.format,
            Some(args.input.display().to_string()),
            idx + 1,
            warnings,
        );
        notes.push(note);
    }

    let rendered = render::render_notes(&notes, args.out_format, config.csv.layout)?;
    util::write_string(&args.out, &rendered)?;
    Ok(())
}

fn run_batch_command(args: &BatchArgs) -> Result<()> {
    let config = Config::load(args.config.as_deref())?;
    let report = run_batch(args, &config)?;
    let report_path = args.out_dir.join("batch_report.json");
    report.write_to(&report_path)?;
    Ok(())
}

pub fn run_batch(args: &BatchArgs, config: &Config) -> Result<BatchReport> {
    let start = Instant::now();
    let mut report = BatchReport::new("clinote");
    std::fs::create_dir_all(&args.out_dir)?;

    let glob_pattern = args
        .glob
        .clone()
        .unwrap_or_else(|| config.glob_default.clone());
    let pattern = args.input_dir.join(glob_pattern);
    let pattern_str = pattern
        .to_str()
        .ok_or_else(|| anyhow!("Invalid glob pattern"))?
        .to_string();

    let bundle_mode = args.bundle.unwrap_or(config.bundle.mode_default);

    for entry in glob(&pattern_str)? {
        match entry {
            Ok(path) => {
                let file_result = process_file(&path, args, config, bundle_mode);
                match file_result {
                    Ok(notes) => {
                        report.record_ok(&notes);
                    }
                    Err(err) => {
                        report.record_failure(&path.display().to_string(), err.to_string());
                    }
                }
            }
            Err(err) => {
                report.record_failure("glob", err.to_string());
            }
        }
    }

    report.finalize();
    report.runtime_ms = start.elapsed().as_millis();
    Ok(report)
}

fn process_file(
    path: &Path,
    args: &BatchArgs,
    config: &Config,
    bundle_mode: BundleMode,
) -> Result<Vec<crate::models::StructuredNote>> {
    let content = util::read_to_string(path)?;
    let (note_texts, bundle_warnings) = parser::split_bundle(&content, bundle_mode, config);
    let mut notes = Vec::new();
    for (idx, note_text) in note_texts.iter().enumerate() {
        let (candidates, mut warnings) = parser::extract_candidates(
            note_text,
            args.format,
            config,
            ParseOptions {
                apply_heuristics: config.enable_fallback_heuristics,
            },
        );
        warnings.extend(bundle_warnings.clone());
        let note = parser::build_note(
            candidates,
            args.format,
            Some(path.display().to_string()),
            idx + 1,
            warnings,
        );
        notes.push(note);
    }

    let rendered = render::render_notes(&notes, args.out_format, config.csv.layout)?;
    let stem = util::file_stem(path);
    let out_path = args
        .out_dir
        .join(format!("{}.{}", stem, args.out_format.extension()));
    util::write_string(&out_path, &rendered)?;
    Ok(notes)
}

fn run_sample(args: &SampleArgs) -> Result<()> {
    samples::generate_samples(&args.out_dir, args.n, args.bundles.unwrap_or(0))
}

fn run_validate(args: &ValidateArgs) -> Result<()> {
    if let Some(input) = &args.input {
        let template = args.template.unwrap_or(Template::Soap);
        let config = Config::load(args.config.as_deref())?;
        let input_text = util::read_to_string(input)?;
        let (note_texts, bundle_warnings) =
            parser::split_bundle(&input_text, config.bundle.mode_default, &config);
        let mut reports = Vec::new();
        let mut has_error = false;

        for (idx, note_text) in note_texts.iter().enumerate() {
            let (candidates, mut warnings) = parser::extract_candidates(
                note_text,
                template_to_format(template),
                &config,
                ParseOptions {
                    apply_heuristics: config.enable_fallback_heuristics,
                },
            );
            warnings.extend(bundle_warnings.clone());
            let note = parser::build_note(
                candidates,
                template_to_format(template),
                Some(input.display().to_string()),
                idx + 1,
                warnings,
            );
            let issues = validate::validate_note(&note, template, args.strict);
            if issues.iter().any(|i| i.severity == Severity::Error) {
                has_error = true;
            }
            reports.push(ValidationReport {
                note_index: idx + 1,
                issues,
            });
        }

        if args.json {
            let payload = ValidationSummary {
                input: input.display().to_string(),
                template,
                strict: args.strict,
                reports,
            };
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            print_validation_text(&reports);
        }

        if has_error {
            process::exit(2);
        }
        return Ok(());
    }

    if let Some(config_path) = &args.config {
        let config = Config::load(Some(config_path))?;
        println!("{}", config.summary());
        return Ok(());
    }

    Err(anyhow!(
        "Provide an input file to validate or use --config to validate a config file"
    ))
}

fn run_preview(args: &PreviewArgs) -> Result<()> {
    let config = Config::load(args.config.as_deref())?;
    let template = args.template.unwrap_or(Template::Soap);
    let input_text = util::read_to_string(&args.input)?;
    let (note_texts, _warnings) =
        parser::split_bundle(&input_text, config.bundle.mode_default, &config);

    for (idx, note_text) in note_texts.iter().enumerate() {
        let (candidates, _) = parser::extract_candidates(
            note_text,
            template_to_format(template),
            &config,
            ParseOptions {
                apply_heuristics: config.enable_fallback_heuristics,
            },
        );
        let note = parser::build_note(
            candidates,
            template_to_format(template),
            Some(args.input.display().to_string()),
            idx + 1,
            Vec::new(),
        );
        println!("Note {}:", idx + 1);
        for summary in validate::summarize_sections(&note) {
            println!(
                "- {}: {} lines, {} chars",
                summary.name, summary.line_count, summary.char_count
            );
        }
        if idx + 1 < note_texts.len() {
            println!();
        }
    }
    Ok(())
}

fn run_init(args: &InitArgs) -> Result<()> {
    if args.path.exists() {
        return Err(anyhow!(
            "Config file already exists at {}",
            args.path.display()
        ));
    }
    let template = default_config_template();
    util::write_string(&args.path, &template)?;
    println!("Created default config at {}", args.path.display());
    Ok(())
}

fn run_demo(args: &DemoArgs) -> Result<()> {
    samples::generate_samples(&args.out_dir, 6, 2)?;
    let outputs_dir = args.out_dir.join("converted");
    std::fs::create_dir_all(&outputs_dir)?;

    let config = Config::default();
    let pattern = args.out_dir.join("sample_*.txt");
    let pattern_str = pattern
        .to_str()
        .ok_or_else(|| anyhow!("Invalid demo glob pattern"))?
        .to_string();

    for entry in glob(&pattern_str)? {
        let path = entry?;
        let content = util::read_to_string(&path)?;
        let format = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) if name.contains("_1") || name.contains("_4") => Template::Soap,
            Some(name) if name.contains("_2") || name.contains("_5") => Template::Hp,
            _ => Template::Discharge,
        };
        let (candidates, _) = parser::extract_candidates(
            &content,
            template_to_format(format),
            &config,
            ParseOptions {
                apply_heuristics: config.enable_fallback_heuristics,
            },
        );
        let note = parser::build_note(
            candidates,
            template_to_format(format),
            Some(path.display().to_string()),
            1,
            Vec::new(),
        );
        let rendered =
            render::render_notes(&[note.clone()], OutputFormat::Json, config.csv.layout)?;
        let out_path = outputs_dir.join(format!("{}.json", util::file_stem(&path)));
        util::write_string(&out_path, &rendered)?;

        let issues = validate::validate_note(&note, format, false);
        let report_path = outputs_dir.join(format!("{}.validation.json", util::file_stem(&path)));
        util::write_string(&report_path, &serde_json::to_string_pretty(&issues)?)?;
    }

    println!("Demo outputs written to {}", outputs_dir.display());
    Ok(())
}

fn run_selftest(args: &SelftestArgs) -> Result<()> {
    let template = args.template.unwrap_or(Template::Soap);
    let out_dir = args.out.as_deref();
    let summary = selftest::run_selftest(&args.fixtures, template, args.strict, out_dir)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print!("{}", selftest::summarize_text(&summary));
    }

    if summary.runtime_failures > 0 {
        process::exit(1);
    }
    if summary.total_errors > 0 {
        process::exit(2);
    }
    Ok(())
}

fn template_to_format(template: Template) -> NoteFormat {
    match template {
        Template::Soap => NoteFormat::Soap,
        Template::Hp => NoteFormat::Hp,
        Template::Discharge => NoteFormat::Discharge,
    }
}

#[derive(Debug, serde::Serialize)]
struct ValidationReport {
    note_index: usize,
    issues: Vec<ValidationIssue>,
}

#[derive(Debug, serde::Serialize)]
struct ValidationSummary {
    input: String,
    template: Template,
    strict: bool,
    reports: Vec<ValidationReport>,
}

fn print_validation_text(reports: &[ValidationReport]) {
    for report in reports {
        println!("Note {}:", report.note_index);
        if report.issues.is_empty() {
            println!("  No issues detected.");
            continue;
        }
        for issue in &report.issues {
            let section = issue
                .section
                .as_ref()
                .map(|s| format!(" [{}]", s))
                .unwrap_or_default();
            println!("  - {:?}: {}{}", issue.severity, issue.message, section);
        }
    }
}

fn default_config_template() -> String {
    let template = r#"# Clinote config template
# Customize section orders, aliases, and bundle delimiters.

[formats.soap]
section_order = ["Subjective", "Objective", "Assessment", "Plan"]

[formats.hp]
section_order = [
  "Chief Complaint",
  "HPI",
  "PMH",
  "Medications",
  "Allergies",
  "ROS",
  "Physical Exam",
  "Assessment",
  "Plan"
]

[formats.discharge]
section_order = [
  "Admission Dx",
  "Discharge Dx",
  "Hospital Course",
  "Medications",
  "Follow-up",
  "Disposition",
  "Instructions"
]

# Map variants to canonical headings.
heading_aliases = { "Hx" = "PMH", "Dx" = "Assessment" }

# Enable heuristic fallbacks for missing headings.
enable_fallback_heuristics = true

[bundle]
mode_default = "auto"
delimiters = ["----- NOTE -----", "=== VISIT ==="]

[csv]
layout = "wide"

glob_default = "*.txt"
"#;
    template.to_string()
}
