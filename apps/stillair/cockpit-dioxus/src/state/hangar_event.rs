#[derive(Debug, Clone, PartialEq)]
pub enum HangarEvent {
    FilesDropped { count: usize },
    AddMoreRequested,
    DetectionTimeout,
    PlatformChosen { platform: String },
    FlavourChosen { flavour: String },
    AnalysisStarted,
    AnalysisComplete,
    Reset,
}
