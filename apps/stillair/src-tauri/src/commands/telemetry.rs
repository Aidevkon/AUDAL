use crate::telemetry_listener::LatestFrame;
use tauri::State;
use serde::Serialize;

#[derive(Serialize)]
pub struct RealtimeFrameJson {
    pub spectrum: Vec<f32>,        // converted from [f32; 64]
    pub gonio_path: Vec<[f32; 2]>, // converted from [(f32,f32); 32]
    pub position_ms: u64,
}

#[tauri::command]
pub fn get_live_telemetry_realtime(latest: State<'_, LatestFrame>) -> Option<RealtimeFrameJson> {
    // INV-TB-6: reads Mutex — zero network hop to UI
    let frame_opt = {
        let mut lock = latest.0.lock().ok()?;
        lock.take()
    };
    
    let frame = frame_opt?;
    
    // Explicit conversion
    Some(RealtimeFrameJson {
        spectrum: frame.spectrum.to_vec(),
        gonio_path: frame.gonio_path.iter().map(|(l, r)| [*l, *r]).collect(),
        position_ms: frame.position_ms,
    })
}
