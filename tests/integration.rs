//! Integration tests for the timeline core library.
//! All test data is fictional.

use chrono::TimeZone;
use chrono::Utc;
use timeline::{
    detect_conflicts, import_csv, parse_timestamp, render_markdown, Conflict, Event, Store,
};

fn event(
    at: &str,
    desc: &str,
    source: &str,
    subject: Option<&str>,
    location: Option<&str>,
) -> Event {
    Event {
        at: parse_timestamp(at).unwrap(),
        description: desc.to_string(),
        source: source.to_string(),
        subject: subject.map(str::to_string),
        location: location.map(str::to_string),
    }
}

#[test]
fn timestamps_parse_rfc3339_and_plain_dates() {
    let dt = parse_timestamp("2026-09-18T14:32:00Z").unwrap();
    assert_eq!(dt, Utc.with_ymd_and_hms(2026, 9, 18, 14, 32, 0).unwrap());

    let date = parse_timestamp("2026-09-18").unwrap();
    assert_eq!(date, Utc.with_ymd_and_hms(2026, 9, 18, 0, 0, 0).unwrap());

    assert!(parse_timestamp("not a date").is_err());
}

#[test]
fn store_sorts_events_chronologically() {
    let store = Store {
        events: vec![
            event("2026-09-18T15:00:00Z", "third", "s1", None, None),
            event("2026-09-18T09:00:00Z", "first", "s1", None, None),
            event("2026-09-18T12:00:00Z", "second", "s1", None, None),
        ],
    };
    let sorted = store.sorted_events();
    let descs: Vec<&str> = sorted.iter().map(|e| e.description.as_str()).collect();
    assert_eq!(descs, vec!["first", "second", "third"]);
}

#[test]
fn csv_import_parses_all_columns() {
    let dir = std::env::temp_dir().join(format!("timeline-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("import.csv");
    std::fs::write(
        &path,
        "at,desc,source,subject,location\n\
         2026-09-18T09:15:00Z,Phone call placed,phone records,Alex Merren,Dockside\n\
         2026-09-18,Seen near pier,witness statement #2,Alex Merren,\n",
    )
    .unwrap();

    let events = import_csv(&path).unwrap();
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].description, "Phone call placed");
    assert_eq!(events[0].source, "phone records");
    assert_eq!(events[0].subject.as_deref(), Some("Alex Merren"));
    assert_eq!(events[0].location.as_deref(), Some("Dockside"));
    // Date-only row normalizes to midnight UTC.
    assert_eq!(
        events[1].at,
        Utc.with_ymd_and_hms(2026, 9, 18, 0, 0, 0).unwrap()
    );
    // Empty location column becomes None.
    assert_eq!(events[1].location, None);
}

#[test]
fn conflicts_flag_impossible_travel() {
    // Same fictional subject at two different fictional locations 10 minutes apart.
    let events = vec![
        event(
            "2026-09-18T09:00:00Z",
            "Debit card used",
            "bank records",
            Some("Alex Merren"),
            Some("Dockside"),
        ),
        event(
            "2026-09-18T09:10:00Z",
            "Toll booth camera hit",
            "highway authority",
            Some("Alex Merren"),
            Some("Ridgeline Pass"),
        ),
    ];
    let conflicts = detect_conflicts(&events, 30);
    assert!(
        conflicts.iter().any(|c| matches!(
            c,
            Conflict::ImpossibleTravel { subject, .. } if subject == "Alex Merren"
        )),
        "expected an ImpossibleTravel conflict, got: {conflicts:?}"
    );
}

#[test]
fn conflicts_flag_duplicate_descriptions_across_sources() {
    let events = vec![
        event(
            "2026-09-18T09:00:00Z",
            "Warehouse door forced",
            "patrol log",
            None,
            None,
        ),
        event(
            "2026-09-18T09:05:00Z",
            "Warehouse door forced",
            "security camera",
            None,
            None,
        ),
    ];
    let conflicts = detect_conflicts(&events, 30);
    assert!(
        conflicts
            .iter()
            .any(|c| matches!(c, Conflict::DuplicateDescription { .. })),
        "expected a DuplicateDescription conflict, got: {conflicts:?}"
    );
}

#[test]
fn clean_timeline_has_no_conflicts() {
    // Fictional but plausible: same subject, locations 3 hours apart — plenty of travel time.
    let events = vec![
        event(
            "2026-09-18T09:00:00Z",
            "Checked into motel",
            "motel register",
            Some("Alex Merren"),
            Some("Dockside"),
        ),
        event(
            "2026-09-18T12:00:00Z",
            "Fuel purchase",
            "card processor",
            Some("Alex Merren"),
            Some("Ridgeline Pass"),
        ),
        event(
            "2026-09-18T12:30:00Z",
            "Fuel purchase",
            "card processor",
            Some("Jordan Vale"),
            Some("Ridgeline Pass"),
        ),
    ];
    let conflicts = detect_conflicts(&events, 30);
    assert!(
        conflicts.is_empty(),
        "expected no conflicts, got: {conflicts:?}"
    );
}

#[test]
fn export_produces_markdown() {
    let events = vec![
        event(
            "2026-09-18T09:00:00Z",
            "Checked into motel",
            "motel register",
            Some("Alex Merren"),
            Some("Dockside"),
        ),
        event(
            "2026-09-18T08:00:00Z",
            "Left apartment",
            "door camera",
            None,
            None,
        ),
    ];
    let md = render_markdown(&events, "Evidence Timeline");
    assert!(md.starts_with("# Evidence Timeline"));
    assert!(md.contains("- **2026-09-18T08:00:00+00:00** — Left apartment *(source: door camera)*"));
    assert!(md.contains("*(source: motel register)*"));
    assert!(md.contains("subject: `Alex Merren`"));
    assert!(md.contains("location: `Dockside`"));
    // Sorted: the 08:00 event must appear before the 09:00 event.
    let first = md.find("Left apartment").unwrap();
    let second = md.find("Checked into motel").unwrap();
    assert!(first < second);
}
