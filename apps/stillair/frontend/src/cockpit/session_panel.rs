//! SessionPanel — Left MFD. FM0–FM2 content.
//! Authority: Phase 5 task-decomposition P5-005 · state-machine.md §7
//!
//! FM0:    Drop zone — "Drop audio file here"
//! FM1:    File name + metadata + preset selector
//! FM1.5:  Preset confirmed + "Intent Sealed — Ready to master"
//! FM2:    Animated progress + stage indicator
//! FM3-5:  Mastering complete badge

use leptos::callback::Callable;
use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::AudioMeta;

/// Available presets for Phase 5 (loaded from schema in Phase 6).
pub const PHASE5_PRESETS: &[(&str, &str, &str)] = &[
    ("spotify",     "Spotify",      "−14 LUFS"),
    ("youtube",     "YouTube",      "−14 LUFS"),
    ("apple_music", "Apple Music",  "−16 LUFS"),
    ("tidal",       "Tidal",        "−14 LUFS"),
    ("broadcast",   "Broadcast",    "−23 LUFS"),
];

#[component]
pub fn SessionPanel(
    mode:             ReadSignal<CockpitMode>,
    audio_meta:       ReadSignal<Option<AudioMeta>>,
    selected_preset:  ReadSignal<Option<&'static str>>,
    on_preset_select: UnsyncCallback<&'static str>,
    on_file_drop:     UnsyncCallback<String>,
) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-session">
            <div class="panel-title">
                <span class="panel-dot"></span>
                "THE SESSION"
            </div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::Idle => view! {
                        <DropZone on_drop=on_file_drop />
                    }.into_any(),

                    CockpitMode::FileLoaded => view! {
                        <FileInfo
                            meta=audio_meta
                            selected=selected_preset
                            on_select=on_preset_select
                        />
                    }.into_any(),

                    CockpitMode::PresetSelected => view! {
                        <PresetConfirmed
                            meta=audio_meta
                            preset=selected_preset
                        />
                    }.into_any(),

                    CockpitMode::Mastering => view! {
                        <MasteringProgress />
                    }.into_any(),

                    _ => view! {
                        <MasteringComplete preset=selected_preset />
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

#[component]
fn DropZone(on_drop: UnsyncCallback<String>) -> impl IntoView {
    let (is_drag_over, set_drag_over) = signal(false);

    view! {
        <div
            class=move || if is_drag_over.get() {
                "drop-zone dragover"
            } else {
                "drop-zone"
            }
            id="drop-zone"
            on:dragover=move |ev| {
                ev.prevent_default();
                set_drag_over.set(true);
            }
            on:dragleave=move |_| {
                set_drag_over.set(false);
            }
            on:drop=move |ev| {
                ev.prevent_default();
                set_drag_over.set(false);
                // Phase 6: real file path from DataTransfer
                // Phase 5: simulate with stub path
                on_drop.run("/tmp/stub_track.wav".to_string());
            }
        >
            <div class="drop-zone-icon">"⬦"</div>
            <div class="drop-zone-label">"Drop Audio File Here"</div>
            <div class="drop-zone-hint">"WAV · AIFF · FLAC up to 192kHz/32-bit"</div>
        </div>
    }
}

#[component]
fn FileInfo(
    meta:      ReadSignal<Option<AudioMeta>>,
    selected:  ReadSignal<Option<&'static str>>,
    on_select: UnsyncCallback<&'static str>,
) -> impl IntoView {
    view! {
        <div class="file-info">
            {move || meta.get().map(|m| view! {
                <div class="file-name">{m.name.clone()}</div>
                <div class="meta-grid">
                    <div class="meta-item">
                        <span class="meta-label">"Format"</span>
                        <span class="meta-value">{m.format.clone()}</span>
                    </div>
                    <div class="meta-item">
                        <span class="meta-label">"Sample Rate"</span>
                        <span class="meta-value">{format!("{} Hz", m.sample_rate)}</span>
                    </div>
                    <div class="meta-item">
                        <span class="meta-label">"Bit Depth"</span>
                        <span class="meta-value">{format!("{}-bit", m.bit_depth)}</span>
                    </div>
                    <div class="meta-item">
                        <span class="meta-label">"Duration"</span>
                        <span class="meta-value">{
                            let mins = (m.duration_s / 60.0) as u32;
                            let secs = (m.duration_s % 60.0) as u32;
                            format!("{mins}:{secs:02}")
                        }</span>
                    </div>
                </div>
            })}

            <div class="preset-section">
                <div class="section-label">"Select Platform Preset"</div>
                <div class="preset-list">
                    {PHASE5_PRESETS.iter().map(|(id, name, lufs)| {
                        let id_static: &'static str = id;
                        let name = *name;
                        let lufs = *lufs;
                        view! {
                            <button
                                id=format!("preset-{id_static}")
                                class=move || if selected.get() == Some(id_static) {
                                    "preset-btn selected"
                                } else {
                                    "preset-btn"
                                }
                                on:click=move |_| on_select.run(id_static)
                            >
                                <span>{name}</span>
                                <span class="preset-lufs">{lufs}</span>
                            </button>
                        }
                    }).collect_view()}
                </div>
            </div>
        </div>
    }
}

#[component]
fn PresetConfirmed(
    meta:   ReadSignal<Option<AudioMeta>>,
    preset: ReadSignal<Option<&'static str>>,
) -> impl IntoView {
    view! {
        <div class="preset-confirmed">
            <div class="intent-badge">"✓ INTENT SEALED"</div>
            {move || meta.get().map(|m| view! {
                <div class="file-name">{m.name.clone()}</div>
            })}
            <div>
                <div class="meta-label">"Target Platform"</div>
                <div class="selected-preset-display">
                    {move || preset.get().map(|p| {
                        PHASE5_PRESETS.iter()
                            .find(|(id, _, _)| *id == p)
                            .map(|(_, name, lufs)| format!("{name}  {lufs}"))
                            .unwrap_or_else(|| p.to_string())
                    }).unwrap_or_default()}
                </div>
            </div>
            <div class="ready-label">"Ready to master — click MASTER to proceed"</div>
        </div>
    }
}

#[component]
fn MasteringProgress() -> impl IntoView {
    view! {
        <div class="mastering-progress">
            <div class="stage-indicator">"★ DSP PIPELINE ACTIVE"</div>
            <div class="progress-track">
                <div class="progress-fill"></div>
            </div>
            <div class="mastering-label">
                "sp314-dsp processing — please wait…"
            </div>
        </div>
    }
}

#[component]
fn MasteringComplete(preset: ReadSignal<Option<&'static str>>) -> impl IntoView {
    view! {
        <div class="mastering-complete">
            <div class="complete-badge">"✓ GOLDEN BLOB PRODUCED"</div>
            {move || preset.get().map(|p| view! {
                <div class="meta-item">
                    <span class="meta-label">"Preset"</span>
                    <span class="meta-value">{p}</span>
                </div>
            })}
        </div>
    }
}
