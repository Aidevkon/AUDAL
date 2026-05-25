use crate::conditions::ConditionSet;
use crate::router::Router;
use crate::merger::Merger;

pub struct Pipelineforge;

impl Pipelineforge {
    pub fn forge(conditions: &ConditionSet) -> Result<String, ForgeError> {
        let flavors = Router::select(conditions);
        let mut dsp_topologies = Vec::new();

        for flavor in flavors {
            dsp_topologies.push(flavor.build(conditions.sample_rate));
        }

        let merged_topology = Merger::merge(dsp_topologies);

        serde_json::to_string_pretty(&merged_topology)
            .map_err(|e| ForgeError::SerializationError(e.to_string()))
    }

    pub fn selected_flavors(conditions: &ConditionSet) -> Vec<&'static str> {
        Router::select(conditions)
            .into_iter()
            .map(|f| match f {
                crate::flavor::Flavor::LowMidClarity => "LowMidClarity",
                crate::flavor::Flavor::PresenceAndAir => "PresenceAndAir",
                crate::flavor::Flavor::AntiPumpStabilization => "AntiPumpStabilization",
                crate::flavor::Flavor::MonoSafeMaster => "MonoSafeMaster",
                crate::flavor::Flavor::LufsNormalization => "LufsNormalization",
                crate::flavor::Flavor::DcRemoval => "DcRemoval",
                crate::flavor::Flavor::HumRemoval => "HumRemoval",
            })
            .collect()
    }
}

#[derive(Debug)]
pub enum ForgeError {
    MergeError(String),
    SerializationError(String),
}
