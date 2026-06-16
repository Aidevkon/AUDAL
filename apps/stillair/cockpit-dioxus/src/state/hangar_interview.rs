#[derive(Debug, Clone, PartialEq)]
pub enum HangarInterviewState {
    AwaitingDrop,
    Detection { track_count: usize },
    AwaitingMore { track_count: usize },
    PlatformCard { selected: Option<String> },
    FlavourCard { selected: Option<String> },
    Ignition { platform: String, flavour: String },
    Analysing,
    Ready,
}
