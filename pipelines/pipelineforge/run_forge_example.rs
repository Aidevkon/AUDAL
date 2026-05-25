use pipelineforge::conditions::{ConditionSet, EngineerCondition};
use pipelineforge::forge::Pipelineforge;

fn main() {
    let mut conditions = ConditionSet {
        conditions: vec![EngineerCondition::MuddyMix, EngineerCondition::PumpDrift],
        sample_rate: 48000,
        target_lufs: -14.0,
    };
    let json = Pipelineforge::forge(&conditions).unwrap();
    println!("{}", json);
}
