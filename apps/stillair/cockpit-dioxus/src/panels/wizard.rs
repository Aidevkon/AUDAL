//! panels/wizard.rs — Welcome Wizard (Onboarding) · J-P9 Step 2
//! Authority: hangar-onboarding-spec v1.0 · onboarding.schema.json v1.0
//!
//! Full-screen overlay wizard. Runs once on first launch.
//! 3-step adaptive flow:
//!   Music:   Step 1 (Vision) → Step 2 (Taste) → Step 3 (Platform) → Finish
//!   Podcast: Step 1 (Vision) → Step 3 (Platform) → Finish
//!   Neutral: Step 1 (Vision) → Finish
//!
//! v1.0: Intermediate copy for all personas. No dismiss button.

use dioxus::prelude::*;

// ── Local types (mirrors lineos-types/onboarding.rs) ─────────────────────────
// Defined locally to maintain cockpit WASM isolation (A-002 §3).

#[derive(Debug, Clone, PartialEq, Default)]
pub enum WizardState {
    #[default]
    Initial,
    VisionSelected,
    TasteSet,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Vision {
    #[default]
    Neutral,
    Music,
    Podcast,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TasteProfile {
    pub warmth:     f32,
    pub clarity:    f32,
    pub punch:      f32,
    pub brightness: f32,
}

impl Default for TasteProfile {
    fn default() -> Self {
        Self { warmth: 0.5, clarity: 0.5, punch: 0.5, brightness: 0.5 }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum PlatformTarget {
    Spotify,
    Youtube,
    Broadcast,
    #[default]
    Undecided,
}

#[derive(Debug, Clone)]
pub struct OnboardingState {
    pub wizard_state:   WizardState,
    pub vision:         Vision,
    pub taste:          TasteProfile,
    pub platform_target: PlatformTarget,
    pub completed_at:   Option<u64>,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            wizard_state:    WizardState::Initial,
            vision:          Vision::default(),
            taste:           TasteProfile::default(),
            platform_target: PlatformTarget::default(),
            completed_at:    None,
        }
    }
}

// ── WelcomeWizard ─────────────────────────────────────────────────────────────

#[component]
pub fn WelcomeWizard(
    on_complete: EventHandler<OnboardingState>,
) -> Element {
    let mut state = use_signal(OnboardingState::default);
    let mut step  = use_signal(|| 1_u8);

    rsx! {
        div {
            style: "position:fixed; inset:0; z-index:9999;
                    background:var(--bg-primary, #0a0a0f);
                    display:flex; align-items:center; justify-content:center;
                    font-family:var(--font-mono, monospace);",

            div {
                style: "width:100%; max-width:520px; padding:2.5rem;
                        border:1px solid var(--border-subtle, #1a1a2e);
                        background:var(--bg-secondary, #0f0f1a);",

                // ── Step indicator ────────────────────────────────
                div {
                    style: "display:flex; gap:6px; margin-bottom:2rem;",
                    for i in 1..=total_steps(&state.read().vision) {
                        {
                            let current = *step.read();
                            let is_active = i == current;
                            let is_done   = i < current;
                            rsx! {
                                div {
                                    style: format!(
                                        "height:2px; flex:1; transition:background 0.3s; background:{};",
                                        if is_active { "var(--accent-cyan, #00e5ff)" }
                                        else if is_done { "var(--accent-cyan, #00e5ff)" }
                                        else { "var(--border-subtle, #1a1a2e)" }
                                    ),
                                }
                            }
                        }
                    }
                }

                // ── Step content ──────────────────────────────────
                { match *step.read() {
                    1 => rsx! {
                        StepVision {
                            on_select: move |v: Vision| {
                                state.write().vision = v.clone();
                                state.write().wizard_state = WizardState::VisionSelected;
                                match v {
                                    Vision::Music   => step.set(2),
                                    Vision::Podcast => step.set(3),
                                    Vision::Neutral => {
                                        let mut s = state.write();
                                        s.wizard_state = WizardState::Completed;
                                        s.completed_at = Some(now_unix());
                                        drop(s);
                                        on_complete.call(state.read().clone());
                                    }
                                }
                            },
                        }
                    },
                    2 => rsx! {
                        StepTaste {
                            taste: state.read().taste.clone(),
                            on_next: move |t: TasteProfile| {
                                state.write().taste = t;
                                state.write().wizard_state = WizardState::TasteSet;
                                step.set(3);
                            },
                        }
                    },
                    3 => rsx! {
                        StepPlatform {
                            vision: state.read().vision.clone(),
                            on_select: move |p: PlatformTarget| {
                                let mut s = state.write();
                                s.platform_target = p;
                                s.wizard_state = WizardState::Completed;
                                s.completed_at = Some(now_unix());
                                drop(s);
                                on_complete.call(state.read().clone());
                            },
                        }
                    },
                    _ => rsx! {}
                } }
            }
        }
    }
}

// ── Step 1: Vision ────────────────────────────────────────────────────────────

#[component]
fn StepVision(on_select: EventHandler<Vision>) -> Element {
    rsx! {
        div {
            StepHeader {
                title: "What kind of content do you work on?",
                body:  "This helps the system select the right preset.",
            }

            div {
                style: "display:flex; flex-direction:column; gap:12px; margin-top:1.5rem;",

                WizardOption {
                    label: "Music",
                    desc:  "Songs, beats, mixes",
                    onclick: move |_| on_select.call(Vision::Music),
                }
                WizardOption {
                    label: "Podcast",
                    desc:  "Voice, conversations, voice-overs",
                    onclick: move |_| on_select.call(Vision::Podcast),
                }
                WizardOption {
                    label: "Not sure yet",
                    desc:  "Neutral preset — you can change this later",
                    onclick: move |_| on_select.call(Vision::Neutral),
                }
            }
        }
    }
}

// ── Step 2: Taste (music only) ────────────────────────────────────────────────

#[component]
fn StepTaste(
    taste:   TasteProfile,
    on_next: EventHandler<TasteProfile>,
) -> Element {
    let mut warmth     = use_signal(move || taste.warmth);
    let mut clarity    = use_signal(move || taste.clarity);
    let mut punch      = use_signal(move || taste.punch);
    let mut brightness = use_signal(move || taste.brightness);

    rsx! {
        div {
            StepHeader {
                title: "Choose your sonic style.",
                body:  "This influences small aesthetic choices in mastering.",
            }

            div {
                style: "display:flex; flex-direction:column; gap:1.25rem; margin-top:1.5rem;",

                TasteSlider { label: "Warmth",     left: "Cool",   right: "Warm",   value: warmth, on_change: move |v| warmth.set(v) }
                TasteSlider { label: "Clarity",    left: "Dense",  right: "Clean",  value: clarity, on_change: move |v| clarity.set(v) }
                TasteSlider { label: "Punch",      left: "Smooth", right: "Punchy", value: punch, on_change: move |v| punch.set(v) }
                TasteSlider { label: "Brightness", left: "Dark",   right: "Airy",   value: brightness, on_change: move |v| brightness.set(v) }
            }

            div {
                style: "display:flex; gap:12px; margin-top:2rem; justify-content:flex-end;",

                button {
                    style: BUTTON_SECONDARY,
                    onclick: move |_| on_next.call(TasteProfile::default()),
                    "SKIP"
                }
                button {
                    style: BUTTON_PRIMARY,
                    onclick: move |_| on_next.call(TasteProfile {
                        warmth:     *warmth.read(),
                        clarity:    *clarity.read(),
                        punch:      *punch.read(),
                        brightness: *brightness.read(),
                    }),
                    "NEXT →"
                }
            }
        }
    }
}

// ── Step 3: Platform Target ───────────────────────────────────────────────────

#[component]
fn StepPlatform(
    vision:    Vision,
    on_select: EventHandler<PlatformTarget>,
) -> Element {
    rsx! {
        div {
            StepHeader {
                title: "Select platform.",
                body:  "This sets the output loudness standard.",
            }

            div {
                style: "display:flex; flex-direction:column; gap:12px; margin-top:1.5rem;",

                WizardOption {
                    label: "Spotify",
                    desc:  "–14 LUFS target",
                    onclick: move |_| on_select.call(PlatformTarget::Spotify),
                }
                WizardOption {
                    label: "YouTube",
                    desc:  "–14 LUFS target",
                    onclick: move |_| on_select.call(PlatformTarget::Youtube),
                }
                if vision == Vision::Podcast {
                    WizardOption {
                        label: "Broadcast",
                        desc:  "–23 LUFS (EBU R128)",
                        onclick: move |_| on_select.call(PlatformTarget::Broadcast),
                    }
                }
                WizardOption {
                    label: "Not sure",
                    desc:  "Neutral preset",
                    onclick: move |_| on_select.call(PlatformTarget::Undecided),
                }
            }
        }
    }
}

// ── Shared sub-components ─────────────────────────────────────────────────────

#[component]
fn StepHeader(title: &'static str, body: &'static str) -> Element {
    rsx! {
        div {
            div {
                style: "color:var(--text-primary, #e0e0e0); font-size:1.1rem;
                        font-weight:600; letter-spacing:0.05em; margin-bottom:0.5rem;",
                "{title}"
            }
            div {
                style: "color:var(--text-muted, #666); font-size:0.75rem;
                        letter-spacing:0.1em; line-height:1.5;",
                "{body}"
            }
        }
    }
}

#[component]
fn WizardOption(
    label:   &'static str,
    desc:    &'static str,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            style: "display:flex; flex-direction:column; gap:4px;
                    padding:1rem 1.25rem; text-align:left; cursor:pointer;
                    background:transparent;
                    border:1px solid var(--border-subtle, #1a1a2e);
                    color:var(--text-primary, #e0e0e0);
                    font-family:var(--font-mono, monospace);
                    transition:border-color 0.2s, background 0.2s;",
            onmouseenter: move |_| {},
            onclick: move |e| onclick.call(e),

            span {
                style: "font-size:0.8rem; font-weight:600;
                        letter-spacing:0.1em; text-transform:uppercase;",
                "{label}"
            }
            span {
                style: "font-size:0.65rem; color:var(--text-muted, #666);
                        letter-spacing:0.08em;",
                "{desc}"
            }
        }
    }
}

#[component]
fn TasteSlider(
    label:     &'static str,
    left:      &'static str,
    right:     &'static str,
    value:     Signal<f32>,
    on_change: EventHandler<f32>,
) -> Element {
    let pct = (*value.read() * 100.0) as i32;

    rsx! {
        div {
            div {
                style: "display:flex; justify-content:space-between;
                        margin-bottom:4px;",
                span {
                    style: "color:var(--text-muted, #666); font-size:0.55rem;
                            letter-spacing:0.1em; text-transform:uppercase;",
                    "{label}"
                }
            }
            div {
                style: "display:flex; align-items:center; gap:8px;",
                span {
                    style: "color:var(--text-muted, #666); font-size:0.5rem;
                            letter-spacing:0.08em; width:50px; text-align:right;",
                    "{left}"
                }
                input {
                    r#type: "range",
                    min: "0",
                    max: "100",
                    value: "{pct}",
                    style: "flex:1; accent-color:var(--accent-cyan, #00e5ff);
                            height:2px; cursor:pointer;",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f32>() {
                            on_change.call(v / 100.0);
                        }
                    },
                }
                span {
                    style: "color:var(--text-muted, #666); font-size:0.5rem;
                            letter-spacing:0.08em; width:50px;",
                    "{right}"
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn total_steps(vision: &Vision) -> u8 {
    match vision {
        Vision::Music   => 3,
        Vision::Podcast => 2,  // step 1 + step 3
        Vision::Neutral => 1,
    }
}

fn now_unix() -> u64 {
    // WASM-safe: use js_sys if available, else 0
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() / 1000.0) as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

// ── Style constants ──────────────────────────────────────────────────────────

const BUTTON_PRIMARY: &str =
    "font-family:var(--font-mono, monospace); font-size:0.65rem; \
     letter-spacing:0.15em; padding:8px 20px; cursor:pointer; \
     background:transparent; border:1px solid var(--accent-cyan, #00e5ff); \
     color:var(--accent-cyan, #00e5ff); text-transform:uppercase;";

const BUTTON_SECONDARY: &str =
    "font-family:var(--font-mono, monospace); font-size:0.65rem; \
     letter-spacing:0.15em; padding:8px 20px; cursor:pointer; \
     background:transparent; border:1px solid var(--border-subtle, #1a1a2e); \
     color:var(--text-muted, #666); text-transform:uppercase;";
