use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn subscribe_album_events(batch_id: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn(async move {
        use futures_util::StreamExt;
        use reqwest_eventsource::{Event, EventSource};

        let url = format!("http://127.0.0.1:7402/album/{batch_id}/events/stream");
        let mut es = EventSource::get(&url);

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Message(msg)) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&msg.data) {
                        let event_type = val["type"].as_str().unwrap_or("unknown");
                        let tauri_event = format!("album://{event_type}");
                        let _ = app.emit(&tauri_event, &val);

                        // Jini causality: emit to Hangar on fatigue
                        if event_type == "fatigue" {
                            let track = val["track"].as_u64().unwrap_or(1);
                            let prev = val["prev_lufs"].as_f64().unwrap_or(-14.0);
                            let duck = val["ducking"].as_f64().unwrap_or(1.0);
                            let width = val["width"].as_f64().unwrap_or(1.0);
                            let narrative = format!(
                                "Track {} was adjusted because Track {} \
                                 was aggressive ({:.1} LUFS). \
                                 Ducking reduced to {:.2}, width to {:.2}.",
                                track,
                                track.saturating_sub(1),
                                prev,
                                duck,
                                width
                            );
                            let jini_payload = serde_json::json!({
                                "narrative":    narrative,
                                "action_type":  "nothing",
                                "action_label": "",
                                "confidence":   0.9_f64
                            });
                            let _ = app.emit("jini://album_fatigue", &jini_payload);
                        }
                    }
                }
                Err(_) => break,
                _ => {}
            }
        }
    });
    Ok(())
}
