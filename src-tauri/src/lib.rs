//! Timeline desktop. Same local store as the CLI. Nothing leaves the machine.

use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;
use timeline::{
    detect_conflicts, parse_timestamp, render_markdown, Conflict, Event, Store, DEFAULT_MPH,
    DEFAULT_WINDOW_MINUTES,
};

struct Desk {
    store: Store,
    path: PathBuf,
}

#[derive(Serialize)]
struct ConflictLine {
    kind: String,
    text: String,
}

fn lock(state: &Mutex<Desk>) -> Result<std::sync::MutexGuard<'_, Desk>, String> {
    state.lock().map_err(|e| e.to_string())
}

fn save(desk: &Desk) -> Result<(), String> {
    desk.store.save(&desk.path).map_err(|e| e.to_string())
}

#[tauri::command]
fn store_path(state: tauri::State<'_, Mutex<Desk>>) -> Result<String, String> {
    Ok(lock(&state)?.path.display().to_string())
}

#[tauri::command]
fn list_events(state: tauri::State<'_, Mutex<Desk>>) -> Result<Vec<Event>, String> {
    Ok(lock(&state)?.store.sorted_events())
}

#[tauri::command]
fn add_event(
    state: tauri::State<'_, Mutex<Desk>>,
    at: String,
    description: String,
    source: String,
    subject: Option<String>,
    location: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Result<Vec<Event>, String> {
    let at = parse_timestamp(&at).map_err(|e| e.to_string())?;
    let mut desk = lock(&state)?;
    desk.store.events.push(Event {
        at,
        description: description.trim().to_string(),
        source: source.trim().to_string(),
        subject: subject.filter(|s| !s.trim().is_empty()),
        location: location.filter(|s| !s.trim().is_empty()),
        lat,
        lon,
    });
    save(&desk)?;
    Ok(desk.store.sorted_events())
}

#[tauri::command]
fn conflicts(state: tauri::State<'_, Mutex<Desk>>) -> Result<Vec<ConflictLine>, String> {
    let desk = lock(&state)?;
    let found = detect_conflicts(&desk.store.events, DEFAULT_WINDOW_MINUTES);
    Ok(found
        .into_iter()
        .map(|c| match c {
            Conflict::ImpossibleTravel {
                subject,
                first,
                second,
                gap_minutes,
                required_minutes,
                confidence,
            } => {
                let drive = required_minutes
                    .map(|mins| format!(", drive estimate {mins} min at {DEFAULT_MPH} mph"))
                    .unwrap_or_default();
                let rank = match confidence {
                    timeline::Confidence::High => "high",
                    timeline::Confidence::Medium => "medium",
                };
                ConflictLine {
                    kind: "travel".into(),
                    text: format!(
                        "{rank}: {subject} at {} then {} , {gap_minutes} min apart{drive}",
                        first.location.as_deref().unwrap_or("?"),
                        second.location.as_deref().unwrap_or("?"),
                    ),
                }
            }
            Conflict::DuplicateDescription {
                description,
                sources,
            } => ConflictLine {
                kind: "duplicate".into(),
                text: format!("\"{description}\" from {}", sources.join(", ")),
            },
        })
        .collect())
}

#[tauri::command]
fn export_markdown(state: tauri::State<'_, Mutex<Desk>>) -> Result<String, String> {
    let desk = lock(&state)?;
    let md = render_markdown(&desk.store.events, "Evidence Timeline");
    let path = desk.path.with_extension("md");
    fs::write(&path, &md).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            let path = dir.join("timeline.json");
            let store = Store::load(&path)?;
            app.manage(Mutex::new(Desk { store, path }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            store_path,
            list_events,
            add_event,
            conflicts,
            export_markdown
        ])
        .run(tauri::generate_context!())
        .expect("error while running timeline");
}
