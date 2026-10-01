//! Core data model and logic for the `timeline` evidence timeline builder.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Default filename for the timeline store in the current directory.
pub const STORE_FILE: &str = "timeline.json";

/// Default travel threshold for impossible-travel conflict detection.
pub const DEFAULT_WINDOW_MINUTES: i64 = 30;

/// A single evidence event on the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// When the event occurred (always stored normalized to UTC).
    pub at: DateTime<Utc>,
    /// Free-text description of the event.
    pub description: String,
    /// Source label, e.g. "phone records", "witness statement #3".
    pub source: String,
    /// Optional subject tag (person of interest).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// Optional location tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Optional latitude, decimal degrees. Used only for travel-time estimates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    /// Optional longitude, decimal degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
}

/// The on-disk store: a flat JSON document holding all events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Store {
    /// Load the store from a path, or return an empty store if it doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Store::default());
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let store: Store = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(store)
    }

    /// Persist the store to a path (pretty-printed JSON).
    pub fn save(&self, path: &Path) -> Result<()> {
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    /// Events sorted chronologically (stable, ties broken by insertion order).
    pub fn sorted_events(&self) -> Vec<Event> {
        let mut events = self.events.clone();
        events.sort_by_key(|e| e.at);
        events
    }
}

/// Parse a timestamp: RFC3339 (e.g. `2026-09-18T14:32:00Z`) or a plain date
/// (`2026-09-18`, treated as midnight UTC).
pub fn parse_timestamp(input: &str) -> Result<DateTime<Utc>> {
    let trimmed = input.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap()));
    }
    bail!("unrecognized timestamp '{input}': expected RFC3339 or YYYY-MM-DD")
}

/// Default assumed speed when both sightings have coordinates.
pub const DEFAULT_MPH: f64 = 45.0;

/// How hard a conflict should be leaned on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Coordinates say the gap is less than half the required drive.
    High,
    /// Window hit, or coordinates say the drive does not fit, but not by half.
    Medium,
}

/// A flagged conflict between two (or more) events.
#[derive(Debug, Clone, PartialEq)]
pub enum Conflict {
    /// Same subject recorded at two different locations too close together
    /// to travel, either inside the flat window or slower than the drive estimate.
    ImpossibleTravel {
        subject: String,
        first: Event,
        second: Event,
        gap_minutes: i64,
        /// Minutes the drive would take at the assumed speed, when both points have coordinates.
        required_minutes: Option<i64>,
        confidence: Confidence,
    },
    /// Exact-duplicate descriptions reported by different sources.
    DuplicateDescription {
        description: String,
        sources: Vec<String>,
    },
}

/// Scan a set of events for suspicious overlaps.
///
/// `window_minutes` is the flat travel threshold. When both sightings carry
/// coordinates, a drive-time estimate at [`DEFAULT_MPH`] can also flag the pair.
pub fn detect_conflicts(events: &[Event], window_minutes: i64) -> Vec<Conflict> {
    detect_conflicts_at(events, window_minutes, DEFAULT_MPH)
}

/// Same as [`detect_conflicts`], with an explicit assumed speed in miles per hour.
pub fn detect_conflicts_at(events: &[Event], window_minutes: i64, mph: f64) -> Vec<Conflict> {
    let mut conflicts = Vec::new();

    let mut by_subject: HashMap<&str, Vec<&Event>> = HashMap::new();
    for e in events {
        if let Some(subject) = e.subject.as_deref() {
            by_subject.entry(subject).or_default().push(e);
        }
    }
    let window = Duration::minutes(window_minutes);
    for (subject, mut sightings) in by_subject {
        sightings.sort_by_key(|e| e.at);
        for pair in sightings.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let gap = b.at - a.at;
            let different_place = match (&a.location, &b.location) {
                (Some(la), Some(lb)) => !la.trim().eq_ignore_ascii_case(lb.trim()),
                _ => false,
            };
            if !different_place {
                continue;
            }
            let required = travel_minutes(a, b, mph);
            let gap_minutes = gap.num_minutes();
            let window_hit = gap < window;
            let travel_hit = required.is_some_and(|need| gap_minutes < need);
            if window_hit || travel_hit {
                let confidence = match required {
                    Some(need) if gap_minutes * 2 < need => Confidence::High,
                    _ => Confidence::Medium,
                };
                conflicts.push(Conflict::ImpossibleTravel {
                    subject: subject.to_string(),
                    first: a.clone(),
                    second: b.clone(),
                    gap_minutes,
                    required_minutes: required,
                    confidence,
                });
            }
        }
    }

    // Exact-duplicate descriptions from different sources.
    let mut by_desc: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in events {
        by_desc
            .entry(e.description.trim())
            .or_default()
            .push(e.source.trim());
    }
    for (desc, sources) in by_desc {
        let mut unique: Vec<&str> = sources.clone();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() > 1 && !desc.is_empty() {
            conflicts.push(Conflict::DuplicateDescription {
                description: desc.to_string(),
                sources: unique.iter().map(|s| s.to_string()).collect(),
            });
        }
    }

    conflicts
}

/// Render events as a markdown timeline document with source citations.
pub fn render_markdown(events: &[Event], title: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {title}\n\n"));
    let mut sorted: Vec<&Event> = events.iter().collect();
    sorted.sort_by_key(|e| e.at);
    for e in sorted {
        let mut line = format!(
            "- **{}** — {} *(source: {})*",
            e.at.to_rfc3339(),
            e.description,
            e.source
        );
        if let Some(subject) = &e.subject {
            line.push_str(&format!(" — subject: `{subject}`"));
        }
        if let Some(location) = &e.location {
            line.push_str(&format!(" — location: `{location}`"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Parse a CSV file with columns `at,desc,source,subject,location`.
pub fn import_csv(path: &Path) -> Result<Vec<Event>> {
    let mut reader = csv::Reader::from_path(path)
        .with_context(|| format!("failed to open CSV {}", path.display()))?;
    let mut events = Vec::new();
    for (idx, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("bad CSV record at line {}", idx + 2))?;
        let at_raw = record.get(0).unwrap_or("").trim();
        let description = record.get(1).unwrap_or("").trim().to_string();
        let source = record.get(2).unwrap_or("").trim().to_string();
        let subject = non_empty(record.get(3));
        let location = non_empty(record.get(4));
        if at_raw.is_empty() || description.is_empty() || source.is_empty() {
            bail!(
                "CSV line {}: 'at', 'desc' and 'source' columns are required",
                idx + 2
            );
        }
        let at = parse_timestamp(at_raw)
            .with_context(|| format!("CSV line {}: invalid 'at' value", idx + 2))?;
        events.push(Event {
            at,
            description,
            source,
            subject,
            location,
            lat: parse_coord(record.get(5)),
            lon: parse_coord(record.get(6)),
        });
    }
    Ok(events)
}

fn non_empty(field: Option<&str>) -> Option<String> {
    field
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_coord(field: Option<&str>) -> Option<f64> {
    field.map(str::trim).filter(|s| !s.is_empty())?.parse().ok()
}

/// Minutes to drive between two sightings at `mph`, when both have coordinates.
fn travel_minutes(a: &Event, b: &Event, mph: f64) -> Option<i64> {
    if mph <= 0.0 {
        return None;
    }
    let (lat1, lon1) = (a.lat?, a.lon?);
    let (lat2, lon2) = (b.lat?, b.lon?);
    let miles = haversine_miles(lat1, lon1, lat2, lon2);
    Some(((miles / mph) * 60.0).ceil() as i64)
}

fn haversine_miles(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let radius = 3958.8;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    radius * 2.0 * a.sqrt().asin()
}
