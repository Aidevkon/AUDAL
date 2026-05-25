use openclaw::OpenClawEngine;
use pipelineforge::conditions::{ConditionSet, EngineerCondition};
use pipelineforge::forge::Pipelineforge;

fn main() {
    let conditions = ConditionSet {
        conditions: vec![EngineerCondition::MuddyMix],
        target_lufs: -14.0,
        sample_rate: 48000,
    };
    let json = Pipelineforge::forge(&conditions).unwrap();
    println!("{}", json);
}
