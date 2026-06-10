// integration/src/error.rs — FirewallError
// Authority: spec/locked/S-009_integration_firewall.md v1.0

#[derive(Debug, serde::Serialize)]
pub enum FirewallError {
    /// E_MACRO_OUT_OF_RANGE — macro value outside [0,1]
    MacroOutOfRange { field: String, value: f32 },
    /// E_SCHEMA_FAIL — output failed JSON schema validation
    SchemaFail { source: String, detail: String },
    /// E_FIREWALL_CLAMP — value clamped (non-fatal, logged only)
    FirewallClamp {
        field: String,
        original: f32,
        clamped: f32,
    },
}
