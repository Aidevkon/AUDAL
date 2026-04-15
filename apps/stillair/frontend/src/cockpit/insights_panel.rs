//! InsightsPanel — Center MFD.
//! FM0-FM1:  Awaiting…
//! FM2:      Live telemetry (LUFS + peak + progress bar + stage) — P6-007
//! FM3:      Computing metrics…
//! FM4-FM5:  Real EBU R128 metrics from Golden Blob — P6-005/P6-006
//! FM6:      Locked during export

use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::Metrics;

#[component]
pub fn InsightsPanel(
    mode:       ReadSignal<CockpitMode>,
    metrics:    ReadSignal<Option<Metrics>>,
    // FM2 live telemetry — P6-007
    live_lufs:  ReadSignal<Option<f32>>,
    live_peak:  ReadSignal<Option<f32>>,
    progress:   ReadSignal<f32>,
    stage_name: ReadSignal<String>,
) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-insights">
            <div class="panel-title">
                <span class="panel-dot"></span>
                "THE INSIGHTS"
            </div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::Idle
                    | CockpitMode::FileLoaded
                    | CockpitMode::PresetSelected => view! {
                        <div class="status">"Awaiting mastering…"</div>
                    }.into_any(),

                    CockpitMode::Mastering => view! {
                        <LiveTelemetry
                            live_lufs=live_lufs
                            live_peak=live_peak
                            progress=progress
                            stage_name=stage_name
                        />
                    }.into_any(),

                    CockpitMode::Mastered => view! {
                        <div class="status">"Computing metrics…"</div>
                    }.into_any(),

                    CockpitMode::InsightsReady
                    | CockpitMode::CoachReady
                    | CockpitMode::Exporting => view! {
                        <MetricsDisplay metrics=metrics />
                    }.into_any(),

                    CockpitMode::Fault(_) => view! {
                        <div class="status">"FAULT — awaiting reset"</div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

// ── FM2 Live Telemetry (P6-007) ───────────────────────────────────────────────

#[component]
fn LiveTelemetry(
    live_lufs:  ReadSignal<Option<f32>>,
    live_peak:  ReadSignal<Option<f32>>,
    progress:   ReadSignal<f32>,
    stage_name: ReadSignal<String>,
) -> impl IntoView {
    view! {
        <div class="mastering-progress">
            <div class="stage-indicator">
                "★ "{move || stage_name.get()}
            </div>

            // Progress bar — real fill when telemetry arrives, animated sweep otherwise
            <div class="progress-track">
                {move || {
                    let pct = progress.get();
                    if pct > 0.0 {
                        view! {
                            <div
                                class="progress-fill-real"
                                style=move || format!("width: {}%", pct * 100.0)
                            />
                        }.into_any()
                    } else {
                        view! { <div class="progress-fill" /> }.into_any()
                    }
                }}
            </div>

            // Live LUFS display
            {move || live_lufs.get().map(|lufs| view! {
                <div>
                    <div class="live-lufs-label">"Live LUFS"</div>
                    <div class="live-lufs">{format!("{lufs:.1} LUFS")}</div>
                </div>
            })}

            // Live peak display
            {move || live_peak.get().map(|peak| view! {
                <div>
                    <div class="live-lufs-label">"True Peak"</div>
                    <div class="live-lufs">{format!("{peak:.1} dBTP")}</div>
                </div>
            })}

            <div class="mastering-label">"DSP pipeline active — please wait…"</div>
        </div>
    }
}

// ── FM4+FM5 Metrics Display (P6-005) ─────────────────────────────────────────

#[component]
fn MetricsDisplay(metrics: ReadSignal<Option<Metrics>>) -> impl IntoView {
    view! {
        {move || match metrics.get() {
            None => view! { <div class="status">"No metrics available"</div> }.into_any(),
            Some(m) => view! {
                <div class="metrics-display">
                    <MetricRow
                        label="Integrated LUFS"
                        value=format!("{:.1} LUFS", m.integrated_lufs)
                        fill=lufs_fill(m.integrated_lufs)
                    />
                    <MetricRow
                        label="True Peak"
                        value=format!("{:.1} dBTP", m.true_peak_dbtp)
                        fill=peak_fill(m.true_peak_dbtp)
                    />
                    <MetricRow
                        label="Loudness Range"
                        value=format!("{:.1} LU", m.lra)
                        fill=(m.lra / 20.0).min(1.0)
                    />
                    <MetricRow
                        label="Dynamic Range"
                        value=format!("{:.1} dB", m.dynamic_range_db)
                        fill=(m.dynamic_range_db / 20.0).min(1.0)
                    />
                    <MetricRow
                        label="Stereo Correlation"
                        value=format!("{:.2}", m.stereo_correlation)
                        fill=((m.stereo_correlation + 1.0) / 2.0).max(0.0)
                    />

                    // Platform compliance flags — P6-005
                    <div class="compliance-flags">
                        <div class="meta-label">"Platform Compliance"</div>
                        <ComplianceFlag label="Spotify"       pass=m.spotify_compliant />
                        <ComplianceFlag label="YouTube"       pass=m.youtube_compliant />
                        <ComplianceFlag label="Apple Music"   pass=m.apple_music_compliant />
                        <ComplianceFlag label="Broadcast"     pass=m.broadcast_compliant />
                        <ComplianceFlag label="Tidal"         pass=m.tidal_compliant />
                    </div>
                </div>
            }.into_any(),
        }}
    }
}

#[component]
fn MetricRow(label: &'static str, value: String, fill: f32) -> impl IntoView {
    let fill_pct = format!("{}%", (fill * 100.0) as u32);
    view! {
        <div class="metric-row">
            <div class="metric-label">{label}</div>
            <div class="metric-value">{value}</div>
            <div class="metric-bar">
                <div class="metric-bar-fill" style=format!("width: {fill_pct}") />
            </div>
        </div>
    }
}

#[component]
fn ComplianceFlag(label: &'static str, pass: bool) -> impl IntoView {
    view! {
        <div class="flag-row">
            <span>{label}</span>
            <span class=if pass { "flag-pass" } else { "flag-fail" }>
                {if pass { "✓ PASS" } else { "✗ FAIL" }}
            </span>
        </div>
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Normalized fill 0..1 for LUFS bar (maps -30 to -6 range).
fn lufs_fill(lufs: f32) -> f32 {
    ((lufs + 30.0) / 24.0).clamp(0.0, 1.0)
}

/// Normalized fill for TP bar (maps -6 to 0 dBTP).
fn peak_fill(tp: f32) -> f32 {
    ((tp + 6.0) / 6.0).clamp(0.0, 1.0)
}
