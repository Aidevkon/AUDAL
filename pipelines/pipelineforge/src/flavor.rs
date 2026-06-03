use sp314_nodes::topology::DspTopology;
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub enum Flavor {
    LowMidClarity,
    PresenceAndAir,
    AntiPumpStabilization,
    MonoSafeMaster,
    LufsNormalization,
    DcRemoval,
    HumRemoval,
}

impl Flavor {
    pub fn build(&self, _sample_rate: u32) -> DspTopology {
        let json_str = match self {
            Flavor::LowMidClarity => json!({
                "topology_id": "low_mid_clarity",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "biquad_lowmid", "node_type": "BiquadFilter", "parameters": { "filter_type": 3.0, "freq_hz": 250.0, "q": 1.4, "gain_db": 0.0 } },
                    { "node_id": "rms_detector", "node_type": "RMS_Detector", "parameters": { "threshold_db": -18.0, "attack_ms": 10.0, "release_ms": 150.0 } },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "biquad_lowmid", "modulation_type": "audio" },
                    { "source": "Input", "target": "rms_detector", "modulation_type": "audio" },
                    { "source": "rms_detector", "target": "biquad_lowmid", "modulation_type": "parameter", "source_output": "gain_reduction", "target_parameter": "gain_db" },
                    { "source": "biquad_lowmid", "target": "Output", "modulation_type": "audio" }
                ]
            }),
            Flavor::PresenceAndAir => json!({
                "topology_id": "presence_and_air",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "harmonic_gain", "node_type": "Gain", "parameters": { "gain": 1.0 } },
                    { "node_id": "biquad_deess", "node_type": "BiquadFilter", "parameters": { "filter_type": 3.0, "freq_hz": 6000.0, "q": 0.7, "gain_db": 0.0 } },
                    { "node_id": "rms_hf", "node_type": "RMS_Detector", "parameters": { "threshold_db": -24.0, "attack_ms": 1.0, "release_ms": 50.0 } },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "harmonic_gain", "modulation_type": "audio" },
                    { "source": "Input", "target": "biquad_deess", "modulation_type": "audio" },
                    { "source": "Input", "target": "rms_hf", "modulation_type": "audio" },
                    { "source": "rms_hf", "target": "biquad_deess", "modulation_type": "parameter", "source_output": "gain_reduction", "target_parameter": "gain_db" },
                    { "source": "harmonic_gain", "target": "Output", "modulation_type": "audio" },
                    { "source": "biquad_deess", "target": "Output", "modulation_type": "audio" }
                ]
            }),
            Flavor::AntiPumpStabilization => json!({
                "topology_id": "anti_pump",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "compressor_wet", "node_type": "Compressor", "parameters": { "threshold_db": -18.0, "ratio": 4.0 } },
                    { "node_id": "gain_dry", "node_type": "Gain", "parameters": { "gain": 1.0 } },
                    { "node_id": "gain_blend", "node_type": "Gain", "parameters": { "gain": 0.5 } },
                    { "node_id": "limiter_out", "node_type": "Limiter", "parameters": { "ceiling_db": -0.5 } },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "compressor_wet", "modulation_type": "audio" },
                    { "source": "Input", "target": "gain_dry", "modulation_type": "audio" },
                    { "source": "compressor_wet", "target": "gain_blend", "modulation_type": "audio" },
                    { "source": "gain_dry", "target": "gain_blend", "modulation_type": "audio" },
                    { "source": "gain_blend", "target": "limiter_out", "modulation_type": "audio" },
                    { "source": "limiter_out", "target": "Output", "modulation_type": "audio" }
                ]
            }),
            Flavor::MonoSafeMaster => json!({
                "topology_id": "mono_safe",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "ms_encode", "node_type": "MS_Matrix", "parameters": {} },
                    { "node_id": "biquad_side_hp", "node_type": "BiquadFilter", "parameters": { "filter_type": 1.0, "freq_hz": 80.0 } },
                    { "node_id": "ms_decode", "node_type": "Inverse_MS_Matrix", "parameters": {} },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "ms_encode", "modulation_type": "audio" },
                    { "source": "ms_encode", "target": "biquad_side_hp", "modulation_type": "audio" },
                    { "source": "biquad_side_hp", "target": "ms_decode", "modulation_type": "audio" },
                    { "source": "ms_decode", "target": "Output", "modulation_type": "audio" }
                ]
            }),
            Flavor::LufsNormalization => json!({
                "topology_id": "lufs_norm",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "gain_makeup", "node_type": "Gain", "parameters": { "gain": 1.0 } },
                    { "node_id": "ambience_reverb", "node_type": "Reverb", "parameters": {"rt60": 0.0, "hf_damping": 0.5, "diffusion": 0.5, "mix": 0.0} },
                    { "node_id": "ambience_width", "node_type": "Width", "parameters": {"decorrelation": 0.0, "side_gain_db": 0.0, "phase_variance": 0.0, "mono_comp_shelf_db": 0.0} },
                    { "node_id": "limiter_final", "node_type": "Limiter", "parameters": { "ceiling_db": -0.5 } },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "gain_makeup", "modulation_type": "audio" },
                    { "source": "Input", "target": "ambience_reverb", "modulation_type": "audio" },
                    { "source": "ambience_reverb", "target": "ambience_width", "modulation_type": "audio" },
                    { "source": "ambience_width", "target": "limiter_final", "modulation_type": "audio" },
                    { "source": "gain_makeup", "target": "limiter_final", "modulation_type": "audio" },
                    { "source": "limiter_final", "target": "Output", "modulation_type": "audio" }
                ]
            }),
            Flavor::DcRemoval => json!({
                "topology_id": "dc_removal",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "biquad_dc", "node_type": "BiquadFilter", "parameters": { "filter_type": 1.0, "freq_hz": 5.0, "q": 0.707 } },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "biquad_dc", "modulation_type": "audio" },
                    { "source": "biquad_dc", "target": "Output", "modulation_type": "audio" }
                ]
            }),
            Flavor::HumRemoval => json!({
                "topology_id": "hum_removal",
                "nodes": [
                    { "node_id": "Input", "node_type": "Input", "parameters": {} },
                    { "node_id": "notch_50", "node_type": "BiquadFilter", "parameters": { "filter_type": 2.0, "freq_hz": 50.0, "q": 20.0 } },
                    { "node_id": "notch_100", "node_type": "BiquadFilter", "parameters": { "filter_type": 2.0, "freq_hz": 100.0, "q": 20.0 } },
                    { "node_id": "notch_150", "node_type": "BiquadFilter", "parameters": { "filter_type": 2.0, "freq_hz": 150.0, "q": 20.0 } },
                    { "node_id": "Output", "node_type": "Output", "parameters": {} }
                ],
                "edges": [
                    { "source": "Input", "target": "notch_50", "modulation_type": "audio" },
                    { "source": "notch_50", "target": "notch_100", "modulation_type": "audio" },
                    { "source": "notch_100", "target": "notch_150", "modulation_type": "audio" },
                    { "source": "notch_150", "target": "Output", "modulation_type": "audio" }
                ]
            })
        }.to_string();

        DspTopology::from_json(&json_str).unwrap()
    }
}
