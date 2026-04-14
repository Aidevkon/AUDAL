//! TransportBar — state-aware control strip.
//! Authority: Phase 5 task-decomposition P5-004 · state-machine.md §6.4
//!
//! Button states per mode:
//!   MASTER  — enabled only in PresetSelected (FM1.5)
//!   ABORT   — enabled only in Mastering (FM2)
//!   EXPORT  — enabled only in CoachReady (FM5)
//!   LOAD NEW — disabled in Mastering, Exporting, Fault

use leptos::callback::Callable;
use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;

#[component]
pub fn TransportBar(
    mode:        ReadSignal<CockpitMode>,
    on_master:   UnsyncCallback<()>,
    on_abort:    UnsyncCallback<()>,
    on_load_new: UnsyncCallback<()>,
    on_export:   UnsyncCallback<()>,
) -> impl IntoView {
    let can_master  = move || matches!(mode.get(), CockpitMode::PresetSelected);
    let can_abort   = move || matches!(mode.get(), CockpitMode::Mastering);
    let can_export  = move || matches!(mode.get(), CockpitMode::CoachReady);
    let is_locked   = move || matches!(mode.get(),
        CockpitMode::Mastering | CockpitMode::Exporting | CockpitMode::Fault(_));
    let mode_label  = move || mode.get().label();

    view! {
        <div class="transport-bar">
            <span class="transport-id">"STILL AIR"</span>
            <span class="transport-mode">{mode_label}</span>

            <button
                id="btn-master"
                class="transport-btn btn-master"
                disabled=move || !can_master()
                on:click=move |_| on_master.run(())
            >
                {move || if matches!(mode.get(), CockpitMode::Mastering) {
                    "MASTERING…"
                } else {
                    "MASTER"
                }}
            </button>

            <button
                id="btn-abort"
                class="transport-btn btn-abort"
                disabled=move || !can_abort()
                on:click=move |_| on_abort.run(())
            >
                "ABORT"
            </button>

            <button
                id="btn-export"
                class="transport-btn btn-export"
                disabled=move || !can_export()
                on:click=move |_| on_export.run(())
            >
                "EXPORT"
            </button>

            <button
                id="btn-load-new"
                class="transport-btn btn-new"
                disabled=is_locked
                on:click=move |_| on_load_new.run(())
            >
                "LOAD NEW"
            </button>
        </div>
    }
}
