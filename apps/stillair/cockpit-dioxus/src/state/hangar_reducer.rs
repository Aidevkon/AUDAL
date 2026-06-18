use super::hangar_event::HangarEvent;
use super::hangar_interview::HangarInterviewState;
use dioxus::prelude::*;

pub fn dispatch_hangar(
    mut state: Signal<HangarInterviewState>,
    event: HangarEvent,
) {
    web_sys::console::error_1(&format!("[TRAP] HANGAR_DISPATCH: event={:?} state={:?}", event, *state.read()).into());
    let current = state.read().clone();
    let next = reduce_hangar(current, event);
    web_sys::console::error_1(&format!("[TRAP] HANGAR_DISPATCH RESULT: next_state={:?}", next).into());
    state.set(next);
}

pub fn reduce_hangar(
    state: HangarInterviewState,
    event: HangarEvent,
) -> HangarInterviewState {
    use HangarInterviewState::*;
    use HangarEvent::*;
    match (state, event) {
        (AwaitingDrop, FilesDropped { count }) => 
            Detection { track_count: count },

        (Detection { track_count }, AddMoreRequested) =>
            AwaitingMore { track_count },

        (Detection { .. }, DetectionTimeout) |
        (AwaitingMore { .. }, DetectionTimeout) =>
            PlatformCard { selected: None },

        (PlatformCard { .. }, PlatformChosen { platform }) =>
            FlavourCard { platform, track_count: 1 },

        (FlavourCard { platform, .. }, FlavourChosen { flavour }) =>
            Ignition { platform, flavour },

        (Ignition { .. }, AnalysisStarted) => Analysing,

        (Analysing, AnalysisComplete) => Ready,

        (_, Reset) => AwaitingDrop,

        // Illegal transitions — state unchanged
        (state, _) => state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HangarInterviewState::*;

    #[test]
    fn drop_transitions_to_detection() {
        let r = reduce_hangar(AwaitingDrop, 
            HangarEvent::FilesDropped { count: 1 });
        assert!(matches!(r, Detection { track_count: 1 }));
    }

    #[test]
    fn full_happy_path() {
        let mut s = AwaitingDrop;
        s = reduce_hangar(s, HangarEvent::FilesDropped { count: 1 });
        s = reduce_hangar(s, HangarEvent::DetectionTimeout);
        s = reduce_hangar(s, HangarEvent::PlatformChosen { 
            platform: "spotify".into() });
        s = reduce_hangar(s, HangarEvent::FlavourChosen { 
            flavour: "warm".into() });
        s = reduce_hangar(s, HangarEvent::AnalysisStarted);
        s = reduce_hangar(s, HangarEvent::AnalysisComplete);
        assert!(matches!(s, Ready));
    }

    #[test]
    fn illegal_transition_unchanged() {
        let r = reduce_hangar(AwaitingDrop, HangarEvent::AnalysisComplete);
        assert!(matches!(r, AwaitingDrop));
    }
}
