#[derive(Debug, Clone, PartialEq)]
pub enum HangarInterviewState {
    AwaitingDrop,
    Detection { track_count: usize },
    AwaitingMore { track_count: usize },
    PlatformCard,
    FlavourCard,
    Ignition,
    Analysing,
    Ready,
}
