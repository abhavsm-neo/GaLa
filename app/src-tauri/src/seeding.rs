//! Stops sharing a finished download when the chosen rule is met.

use crate::{engine::Engine, settings::Store};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Mutex};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "main";

#[derive(Default)]
pub struct Seeder {
    seeding: Mutex<HashSet<String>>,
    // A rule stops each download once, so pressing Resume on it isn't undone a second later.
    stopped: Mutex<HashSet<String>>,
}

fn rule_met(rule: &str, uploaded: f64, size: f64, seconds: f64) -> bool {
    match rule.split_once(':') {
        Some(("ratio", limit)) => limit.parse::<f64>().is_ok_and(|limit| size > 0.0 && uploaded / size >= limit),
        Some(("hours", limit)) => limit.parse::<f64>().is_ok_and(|limit| seconds >= limit * 3600.0),
        _ => rule == "none",
    }
}

fn pause(app: &AppHandle, id: &str) {
    if let Some(engine) = app.try_state::<Engine>() {
        engine.send(json!({ "cmd": "pause", "id": id }));
    }
}

fn show_count(app: &AppHandle, count: usize) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tip = if count == 0 { "GaLa".to_string() } else { format!("GaLa, seeding {count}") };
        let _ = tray.set_tooltip(Some(tip));
    }
}

pub fn on_engine_event(app: &AppHandle, event: &Value) {
    if event["event"] != "progress" {
        return;
    }
    let seeder = app.state::<Seeder>();
    let id = event["id"].as_str().unwrap_or_default().to_string();
    let active = event["state"] == "seeding" && event["paused"] == false && event["queued"] == false;
    let rule = app.state::<Store>().get().seed_rule;
    let met = active && rule_met(
        &rule,
        event["all_time_upload"].as_f64().unwrap_or(0.0),
        event["total_bytes"].as_f64().unwrap_or(0.0),
        event["seeding_seconds"].as_f64().unwrap_or(0.0),
    );

    let count = {
        let mut seeding = seeder.seeding.lock().unwrap();
        let before = seeding.len();
        if active {
            seeding.insert(id.clone());
        } else {
            seeding.remove(&id);
        }
        (seeding.len() != before).then_some(seeding.len())
    };
    if let Some(count) = count {
        show_count(app, count);
    }
    if met && seeder.stopped.lock().unwrap().insert(id.clone()) {
        pause(app, &id);
    }
}

pub fn stop_all(app: &AppHandle) {
    let ids: Vec<String> = app.state::<Seeder>().seeding.lock().unwrap().iter().cloned().collect();
    for id in ids {
        pause(app, &id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: f64 = 1e9;

    #[test]
    fn ratio_counts_against_the_game_size() {
        assert!(rule_met("ratio:1", 10.0 * GB, 10.0 * GB, 0.0));
        assert!(!rule_met("ratio:1", 9.0 * GB, 10.0 * GB, 0.0));
        assert!(!rule_met("ratio:1", 1.0, 0.0, 0.0));
    }

    #[test]
    fn hours_count_seeding_time() {
        assert!(rule_met("hours:24", 0.0, GB, 86_400.0));
        assert!(!rule_met("hours:24", 0.0, GB, 86_399.0));
    }

    #[test]
    fn forever_never_stops_and_none_always_does() {
        assert!(!rule_met("forever", 100.0 * GB, GB, 1e9));
        assert!(rule_met("none", 0.0, GB, 0.0));
    }
}
