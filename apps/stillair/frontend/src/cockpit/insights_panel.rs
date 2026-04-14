//! InsightsPanel — Center MFD. FM3–FM5 content.
//! Authority: Phase 5 task-decomposition P5-006 · state-machine.md §7
//!
//! FM0-FM2:  "Awaiting mastering…"
//! FM3:      "Computing metrics…"
//! FM4-FM5:  LUFS, TP, LRA, DR, Correlation + compliance flags

use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::Metrics;

#[component]
pub fn InsightsPanel(
    mode:    ReadSignal<CockpitMode>,
    metrics: ReadSignal<Option<Metrics>>,
) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-insights">
            <div class="panel-title">
                <span class="panel-dot"></span>
                "THE INSIGHTS"
            </div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::InsightsReady | CockpitMode::CoachReady => view! {
                        <MetricsDisplay metrics=metrics />
                    }.into_any(),

                    CockpitMode::Mastered => view! {
                        <div class="status">"Computing metrics…"</div>
                    }.into_any(),

                    CockpitMode::Exporting => view! {
                        <div class="locked">"Export in progress"</div>
                    }.into_any(),

                    _ => view! {
                        <div class="status">"Awaiting mastering…"</div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

#[component]
fn MetricsDisplay(metrics: ReadSignal<Option<Metrics>>) -> impl IntoView {
    view! {
        {move || metrics.get().map(|m| view! {
            <div class="metrics-display">
                <LufsGauge value=m.lufs_integrated />
                <MetricRow
                    label="True Peak"
                    value=format!("{:.1} dBTP", m.true_peak)
                    bar_pct=lufs_to_pct(m.true_peak, -6.0, 0.0)
                />
                <MetricRow
                    label="Loudness Range (LRA)"
                    value=format!("{:.1} LU", m.loudness_range)
                    bar_pct=(m.loudness_range / 20.0 * 100.0).min(100.0).max(0.0)
                />
                <MetricRow
                    label="Dynamic Range"
                    value=format!("{:.1} dB", m.dynamic_range)
                    bar_pct=(m.dynamic_range / 20.0 * 100.0).min(100.0).max(0.0)
                />
                <MetricRow
                    label="Stereo Correlation"
                    value=format!("{:.2}", m.stereo_correlation)
                    bar_pct=(m.stereo_correlation * 100.0).min(100.0).max(0.0)
                />
            </div>
        })}
    }
}

#[component]
fn LufsGauge(value: f32) -> impl IntoView {
    let bar_pct = lufs_to_pct(value, -30.0, -6.0);
    view! {
        <div class="metric-row">
            <div class="metric-label">"Integrated Loudness"</div>
            <div class="metric-value">
                {format!("{:.1}", value)}
                <span class="metric-unit">" LUFS"</span>
            </div>
            <div class="metric-bar">
                <div class="metric-bar-fill" style=format!("width: {bar_pct:.1}%")></div>
            </div>
        </div>
    }
}

#[component]
fn MetricRow(label: &'static str, value: String, bar_pct: f32) -> impl IntoView {
    view! {
        <div class="metric-row">
            <div class="metric-label">{label}</div>
            <div class="metric-value" style="font-size: 1rem">{value.clone()}</div>
            <div class="metric-bar">
                <div class="metric-bar-fill" style=format!("width: {bar_pct:.1}%")></div>
            </div>
        </div>
    }
}

/// Map a value in [min, max] to [0, 100] percentage for bar display.
fn lufs_to_pct(v: f32, min: f32, max: f32) -> f32 {
    ((v - min) / (max - min) * 100.0)
        .min(100.0)
        .max(0.0)
}
