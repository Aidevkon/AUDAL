# ΤΑ ΦΑΝΤΑΣΜΑΤΑ ΤΟΥ 'Transparent'

| Αρχείο:Γραμμή | Test | Έλεγχος | Νούμερα; |
| `lineos/m0/m0-daemon/tests/ab_render_full.rs:27` | `render_ab_full_pipeline` | render_ab_full_pipeline | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/audition.rs:176` | `audition` | audition | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/e2e_mastering_quality.rs:83` | `make_req` | make_req | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/glue_full_render.rs:22` | `glue_full_render` | glue_full_render | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/glue_full_render.rs:71` | `glue_full_render` | glue_full_render | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w10_duck_dynamics.rs:108` | `w10_duck_dynamics` | w10_duck_dynamics | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w10_duck_dynamics.rs:161` | `w10_duck_dynamics` | w10_duck_dynamics | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w17_bed_sweep.rs:20` | `render_boost` | render_boost | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w17_mix_balance.rs:19` | `render_with_mix` | render_with_mix | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w1_vad_trace.rs:26` | `render_vad_trace_vehicle` | render_vad_trace_vehicle | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:39` | `w2_duck_gate_synth` | w2_duck_gate_synth | **ΝΑΙ**<br>assert!(posteriors.len() > 1000,<br>assert!(pmax > 0.7,<br>...(+3 ακόμα) |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:83` | `w2_duck_gate_synth` | w2_duck_gate_synth | **ΝΑΙ**<br>assert!(posteriors.len() > 1000,<br>assert!(pmax > 0.7,<br>...(+3 ακόμα) |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:181` | `w2_duck_gate_real` | w2_duck_gate_real | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:226` | `w2_duck_gate_real` | w2_duck_gate_real | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:279` | `w2_duck_gate_synth_variance` | w2_duck_gate_synth_variance | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:349` | `w3b_mix_levels_gate` | w3b_mix_levels_gate | **ΝΑΙ**<br>assert_eq!( |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:449` | `w4_ceiling_gate` | w4_ceiling_gate | **ΝΑΙ**<br>assert!(rms_c > rms_b, "With higher ceiling, PRE-MASTER output RMS should be higher because the gain is not capped at 2.0x"); |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:507` | `w4_ceiling_gate` | w4_ceiling_gate | **ΝΑΙ**<br>assert!(rms_c > rms_b, "With higher ceiling, PRE-MASTER output RMS should be higher because the gain is not capped at 2.0x"); |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:552` | `w4_ceiling_gate` | w4_ceiling_gate | **ΝΑΙ**<br>assert!(rms_c > rms_b, "With higher ceiling, PRE-MASTER output RMS should be higher because the gain is not capped at 2.0x"); |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:630` | `w4_ceiling_sweep` | w4_ceiling_sweep | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w6c_nmfd_cost.rs:22` | `w6c_nmfd_cost` | w6c_nmfd_cost | **ΟΧΙ**<br>- |
| `lineos/m0/m0-daemon/tests/w6c_nmfd_cost.rs:60` | `w6c_nmfd_cost` | w6c_nmfd_cost | **ΟΧΙ**<br>- |

> ΔΙΟΡΘΩΣΗ: μόνο .rs sites — τα ζωντανά literals + το audition
> env default. Οι 2 αφαιρεθείσες γραμμές ήταν αναφορές του northstar.
