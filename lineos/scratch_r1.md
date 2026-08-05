## R1: RenderArtifacts Lifetime Analysis
- **Callers/Holders:** `executor.rs` (receives from `run_dsp`), which immediately moves fields to `DspOutput` and drops `RenderArtifacts` at line 140.
- **Lifetime:** Milliseconds. The HTTP handler (`master.rs`) receives the output, inserts to SurrealDB, and drops it.
- **Size/Risk:** ~11.5 MB per minute of audio per channel. Extending their lifetime by 50ms introduces zero risk of `/tmp` bloat.
- **Design:** Add `pub pre_master_guards: Option<(Arc<ManagedPcm>, Arc<ManagedPcm>)>` to `RenderArtifacts`. We will `Arc::new(ManagedPcm::new(...))` the guards at lines 838 and 856 of `dsp_pipeline.rs`, and return `Some((_scratch_l_guard.clone(), _scratch_r_guard.clone()))` at the end of `run_dsp_internal`.
