// src/node.rs
// The core trait every DSP primitive implements.
// Constitutional: process_stereo() must be zero-allocation.

/// A single DSP processing unit in a node graph.
/// All implementations must be zero-allocation in process_stereo().
pub trait DspNode: Send {
    /// Process one block of stereo audio in-place.
    /// Called by DspGraph for every audio block. Zero allocation.
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]);

    /// Update context features (e.g. NMF
    /// stem energy ratios) before processing.
    /// Nodes needing context (MaskingEQ)
    /// override this. Default: no-op so the
    /// existing 16 nodes are unaffected.
    fn update_features(&mut self, _stem_ratios: &[f32; 5]) {}

    /// Set a named parameter on this node.
    /// Called by parameter modulation edges between blocks.
    /// Unknown parameter names are silently ignored.
    fn set_parameter(&mut self, name: &str, value: f32);

    /// Set a named parameter on this node, but apply it instantly without gliding.
    /// Used by parameter modulation edges.
    fn set_parameter_no_glide(&mut self, name: &str, value: f32) {
        self.set_parameter(name, value);
    }

    /// Read a named output value from this node.
    /// Used by RMS_Detector to expose its envelope to modulation edges.
    /// Returns None if parameter name is unknown.
    fn get_output(&self, name: &str) -> Option<f32>;

    /// Reset all internal state to initial values.
    /// Must produce bit-identical output after reset + same input.
    fn reset(&mut self);

    /// Node type identifier — matches node_type in dsp-topology.schema.json
    fn node_type(&self) -> &'static str;
}
