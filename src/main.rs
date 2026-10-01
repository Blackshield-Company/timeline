//! timeline — local-first evidence timeline builder.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use timeline::{
    detect_conflicts_at, import_csv, parse_timestamp, render_markdown, Confidence, Conflict, Event,
    Store, DEFAULT_MPH, DEFAULT_WINDOW_MINUTES, STORE_FILE,
};

#[derive(Parser)]
#[command(
    name = "timeline",
    version,
    about = "Evidence timeline builder for detectives and defense attorneys"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create an empty timeline.json store in the current directory.
    Init,

    /// Add a single event to the timeline.
    Add {
        /// Timestamp: RFC3339 (2026-09-18T14:32:00Z) or date (2026-09-18).
        #[arg(long, value_name = "TIMESTAMP")]
        at: String,

        /// Description of the event.
        #[arg(long, value_name = "DESC")]
        desc: String,

        /// Source label (e.g. "phone records", "witness statement #3").
        #[arg(long, value_name = "SOURCE")]
        source: String,

        /// Optional subject tag.
        #[arg(long, value_name = "SUBJECT")]
        subject: Option<String>,

        /// Optional location tag.
        #[arg(long, value_name = "LOCATION")]
        location: Option<String>,

        /// Optional latitude, decimal degrees.
        #[arg(long)]
        lat: Option<f64>,

        /// Optional longitude, decimal degrees.
        #[arg(long)]
        lon: Option<f64>,
    },

    /// Bulk import events from a CSV file (columns: at,desc,source,subject,location).
    Import {
        /// Path to the CSV file.
        #[arg(value_name = "FILE.csv")]
        file: PathBuf,
    },

    /// Print the unified, chronologically sorted timeline with source citations.
    Show {
        /// Only show events for this subject.
        #[arg(long, value_name = "SUBJECT")]
        subject: Option<String>,
    },

    /// Flag suspicious overlaps: impossible travel and duplicate descriptions.
    Conflicts {
        /// Travel threshold in minutes for impossible-travel detection.
        #[arg(long, default_value_t = DEFAULT_WINDOW_MINUTES, value_name = "MINUTES")]
        window_minutes: i64,

        /// Assumed driving speed when both sightings have coordinates.
        #[arg(long, default_value_t = DEFAULT_MPH, value_name = "MPH")]
        mph: f64,
    },

    /// Export the timeline as a markdown document (stdout, or a file with --out).
    Export {
        /// Optional output file path. Defaults to stdout.
        #[arg(long, value_name = "FILE.md")]
        out: Option<PathBuf>,
    },
}

fn store_path() -> PathBuf {
    PathBuf::from(STORE_FILE)
}

fn require_store() -> Result<Store> {
    let path = store_path();
    if !path.exists() {
        bail!("no {STORE_FILE} found — run `timeline init` first");
    }
    Store::load(&path)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            let path = store_path();
            if path.exists() {
                bail!("{STORE_FILE} already exists");
            }
            Store::default().save(&path)?;
            println!("created {STORE_FILE}");
        }

        Commands::Add {
            at,
            desc,
            source,
            subject,
            location,
            lat,
            lon,
        } => {
            let at = parse_timestamp(&at).context("invalid --at value")?;
            let mut store = require_store()?;
            store.events.push(Event {
                at,
                description: desc.clone(),
                source,
                subject,
                location,
                lat,
                lon,
            });
            store.save(&store_path())?;
            println!("added event: {desc}");
        }

        Commands::Import { file } => {
            let events = import_csv(&file)?;
            let mut store = require_store()?;
            let count = events.len();
            store.events.extend(events);
            store.save(&store_path())?;
            println!("imported {count} events from {}", file.display());
        }

        Commands::Show { subject } => {
            let store = require_store()?;
            let events = store.sorted_events();
            let filtered: Vec<&Event> = match &subject {
                Some(s) => events
                    .iter()
                    .filter(|e| e.subject.as_deref() == Some(s.as_str()))
                    .collect(),
                None => events.iter().collect(),
            };
            if filtered.is_empty() {
                println!("(no events)");
            } else {
                for e in filtered {
                    let mut line = format!(
                        "{} | {} [source: {}]",
                        e.at.to_rfc3339(),
                        e.description,
                        e.source
                    );
                    if let Some(s) = &e.subject {
                        line.push_str(&format!(" | subject: {s}"));
                    }
                    if let Some(l) = &e.location {
                        line.push_str(&format!(" | location: {l}"));
                    }
                    println!("{line}");
                }
            }
        }

        Commands::Conflicts {
            window_minutes,
            mph,
        } => {
            let store = require_store()?;
            let conflicts = detect_conflicts_at(&store.events, window_minutes, mph);
            if conflicts.is_empty() {
                println!("no conflicts detected");
            } else {
                println!("{} conflict(s) detected:", conflicts.len());
                for c in conflicts {
                    match c {
                        Conflict::ImpossibleTravel {
                            subject,
                            first,
                            second,
                            gap_minutes,
                            required_minutes,
                            confidence,
                        } => {
                            let rank = match confidence {
                                Confidence::High => "high",
                                Confidence::Medium => "medium",
                            };
                            let drive = match required_minutes {
                                Some(mins) => format!(", drive estimate {mins} min at {mph} mph"),
                                None => String::new(),
                            };
                            println!(
                                "- IMPOSSIBLE TRAVEL ({rank}): '{subject}' recorded at '{}' ({}) then at '{}' ({}) only {gap_minutes} min apart (threshold: {window_minutes} min{drive})",
                                first.location.as_deref().unwrap_or("?"),
                                first.at.to_rfc3339(),
                                second.location.as_deref().unwrap_or("?"),
                                second.at.to_rfc3339(),
                            );
                        }
                        Conflict::DuplicateDescription { description, sources } => println!(
                            "- DUPLICATE DESCRIPTION: \"{description}\" reported by different sources: {}",
                            sources.join(", ")
                        ),
                    }
                }
            }
        }

        Commands::Export { out } => {
            let store = require_store()?;
            let md = render_markdown(&store.events, "Evidence Timeline");
            match out {
                Some(path) => {
                    std::fs::write(&path, &md)
                        .with_context(|| format!("failed to write {}", path.display()))?;
                    println!("exported markdown to {}", path.display());
                }
                None => print!("{md}"),
            }
        }
    }

    Ok(())
}
