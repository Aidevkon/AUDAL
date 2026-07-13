use crate::dsp::lazy_reader::LazyAudioReader;
use sp314_dsp::stft::stem_renderer::FiveStems;
use std::path::Path;

/// Job sent from the main render loop to the background NMF worker.
/// Lives in the quarantined `orchestrator` module so it can be
/// lifted into a future standalone crate with minimal rework.
pub struct NmfJob {
    pub segment_id: usize,
    pub start_sec: f32,
    pub duration_sec: f32,
}

/// Result sent back from the worker once separation completes.
pub struct NmfResult {
    pub segment_id: usize,
    pub stems: FiveStems,
}

pub fn spawn(
    file_path: String,
    sample_rate: u32,
    rx: std::sync::mpsc::Receiver<NmfJob>,
    tx: std::sync::mpsc::Sender<NmfResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        // FiveStemRenderer instantiated ONCE for the worker's lifetime
        // (not per-job) — the ~2.2s cost we measured yesterday is paid
        // ONCE here, not per hybrid segment.
        let mut renderer = sp314_dsp::stft::stem_renderer::FiveStemRenderer::new();

        for job in rx {
            // blocks until a job arrives, exits when sender drops
            // Own shadow reader per worker (doesn't touch main thread's reader)
            let mut shadow_reader = match LazyAudioReader::open(Path::new(&file_path)) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("NMF worker: failed to open shadow reader: {:?}", e);
                    continue; // skip this job, don't kill the whole worker
                }
            };

            let start_frame = (job.start_sec * sample_rate as f32) as u64;
            if let Err(e) = shadow_reader.seek_exact_frame(start_frame) {
                eprintln!(
                    "NMF worker: seek failed for segment {}: {:?}",
                    job.segment_id, e
                );
                continue;
            }

            let frames_needed = (job.duration_sec * sample_rate as f32) as u64;
            let (left, right) = match shadow_reader.read_exact_frames_alloc(frames_needed) {
                Ok(lr) => lr,
                Err(e) => {
                    eprintln!(
                        "NMF worker: read failed for segment {}: {:?}",
                        job.segment_id, e
                    );
                    continue;
                }
            };

            // mono downmix for the renderer
            let mono: Vec<f32> = left
                .iter()
                .zip(right.iter())
                .map(|(l, r)| (l + r) * 0.5)
                .collect();

            let stems = renderer.render(&mono);

            if tx
                .send(NmfResult {
                    segment_id: job.segment_id,
                    stems,
                })
                .is_err()
            {
                break; // main thread dropped its receiver, shut down
            }
        }
    })
}

pub fn dispatch_all_jobs(
    boundaries: &[lineos_corpus::scout::SegmentBoundary],
    tx: &std::sync::mpsc::Sender<NmfJob>,
) -> (usize, Vec<usize>) {
    let flagged = lineos_corpus::scout::flag_escalation_candidates(boundaries);
    let mut sent = 0;
    for &idx in &flagged {
        let b = &boundaries[idx];
        let duration = b.end_sec - b.start_sec;
        // send() can fail if the receiver was dropped (worker crashed
        // or shut down early) — don't panic the whole pipeline over
        // one failed dispatch, log and continue
        match tx.send(NmfJob {
            segment_id: idx,
            start_sec: b.start_sec,
            duration_sec: duration,
        }) {
            Ok(()) => sent += 1,
            Err(e) => eprintln!(
                "dispatch_all_jobs: failed to send job for segment {}: {:?}",
                idx, e
            ),
        }
    }
    (sent, flagged) // return count and indices for logging/verification and reuse
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn nmf_job_result_are_channel_safe() {
        let (tx_job, rx_job) = std::sync::mpsc::channel::<NmfJob>();
        let (tx_res, rx_res) = std::sync::mpsc::channel::<NmfResult>();
        tx_job
            .send(NmfJob {
                segment_id: 0,
                start_sec: 0.0,
                duration_sec: 5.0,
            })
            .unwrap();
        let job = rx_job.recv().unwrap();
        assert_eq!(job.segment_id, 0);
        // (just prove it compiles and round-trips; don't need real FiveStems data)
        drop(tx_res);
        drop(rx_res); // exercise the types exist and channel-construct fine
    }

    #[test]
    #[ignore]
    fn test_nmf_worker_e2e_throwaway() {
        let file_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav"
        )
        .to_string();

        let (tx_job, rx_job) = std::sync::mpsc::channel::<NmfJob>();
        let (tx_res, rx_res) = std::sync::mpsc::channel::<NmfResult>();

        let handle = spawn(file_path, 48000, rx_job, tx_res);

        let job = NmfJob {
            segment_id: 42,
            start_sec: 1.0,
            duration_sec: 5.0,
        };
        tx_job.send(job).unwrap();

        let start = std::time::Instant::now();
        let result = rx_res
            .recv_timeout(Duration::from_secs(15))
            .expect("Worker should return result within 15s");
        let elapsed = start.elapsed();

        assert_eq!(result.segment_id, 42);
        assert!(result.stems.voice.len() > 0);
        let expected_frames = (5.0 * 48000.0) as usize;
        // There may be a small truncation in processing chunks, check roughly
        assert!((result.stems.voice.len() as isize - expected_frames as isize).abs() < 2048);

        println!("NMF worker processed 5s segment in {:.2?}", elapsed);

        drop(tx_job);
        handle.join().expect("Thread should join cleanly");
    }

    #[test]
    fn test_dispatch_all_jobs_upfront() {
        use lineos_corpus::scout::{SegmentBoundary, SegmentType};

        // Create a synthetic timeline
        let boundaries = vec![
            // 0: Speech, high confidence -> skip
            SegmentBoundary {
                start_sec: 0.0,
                end_sec: 10.0,
                segment_type: SegmentType::Speech,
                avg_leaning: 0.9,
                avg_confidence: 0.8,
            },
            // 1: Hybrid candidate! leaning in dead zone (0.3-0.7) and confidence < 0.4
            SegmentBoundary {
                start_sec: 10.0,
                end_sec: 20.0,
                segment_type: SegmentType::Speech,
                avg_leaning: 0.5,
                avg_confidence: 0.2,
            },
            // 2: Music, high confidence -> skip
            SegmentBoundary {
                start_sec: 20.0,
                end_sec: 30.0,
                segment_type: SegmentType::Music,
                avg_leaning: 0.1,
                avg_confidence: 0.8,
            },
            // 3: Hybrid candidate!
            SegmentBoundary {
                start_sec: 30.0,
                end_sec: 35.5,
                segment_type: SegmentType::Music,
                avg_leaning: 0.6,
                avg_confidence: 0.3,
            },
            // 4: Speech, high confidence -> skip
            SegmentBoundary {
                start_sec: 35.5,
                end_sec: 50.0,
                segment_type: SegmentType::Speech,
                avg_leaning: 0.85,
                avg_confidence: 0.7,
            },
        ];

        let (tx, rx) = std::sync::mpsc::channel();
        let (sent, flagged) = dispatch_all_jobs(&boundaries, &tx);

        assert_eq!(sent, 2, "Should have dispatched exactly 2 hybrid segments");
        assert_eq!(flagged, vec![1, 3], "Flagged indices should be 1 and 3");

        let job1 = rx.recv().unwrap();
        assert_eq!(job1.segment_id, 1);
        assert_eq!(job1.start_sec, 10.0);
        assert_eq!(job1.duration_sec, 10.0);

        let job2 = rx.recv().unwrap();
        assert_eq!(job2.segment_id, 3);
        assert_eq!(job2.start_sec, 30.0);
        assert_eq!(job2.duration_sec, 5.5);

        // the channel should be empty now
        assert!(rx.try_recv().is_err());
    }
}
