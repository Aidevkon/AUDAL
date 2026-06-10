//! RealtimeBridge — lock-free ring buffer for xaak → UI telemetry.
//! Authority: telemetry-bridge-spec-v1_2.md
//!
//! INV-TB-1: Ring buffer capacity = 4 frames (~2KB)
//! INV-TB-2: push() non-blocking — frame dropped if full
//! INV-TB-3: pop() non-blocking — None if empty
//! INV-TB-4: Audio thread never waits for UI thread
//! INV-TB-5: Auto-disable if UI silent > 2s (AtomicU64)

use lineos_types::RealtimeFrame;
use ringbuf::{HeapConsumer, HeapProducer, HeapRb};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Lock-free ring buffer bridge from xaak audio thread to UI.
/// Producer: xaak audio callback (push, non-blocking)
/// Consumer: Tauri command (pop, non-blocking)
#[derive(Clone)]
pub struct RealtimeBridge {
    producer: Arc<Mutex<HeapProducer<RealtimeFrame>>>,
    consumer: Arc<Mutex<HeapConsumer<RealtimeFrame>>>,
    /// Timestamp (ms since epoch) of last UI poll.
    /// Audio thread checks: if now - last_poll > 2000ms → skip FFT.
    /// Zero tasks, zero allocations — pure atomic.
    last_poll_ms: Arc<AtomicU64>,
}

impl RealtimeBridge {
    pub fn new() -> Self {
        let rb = HeapRb::<RealtimeFrame>::new(4);
        let (producer, consumer) = rb.split();
        Self {
            producer: Arc::new(Mutex::new(producer)),
            consumer: Arc::new(Mutex::new(consumer)),
            last_poll_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Non-blocking push from audio thread.
    /// Returns immediately if UI has not polled in > 2s (bypass).
    /// Drops frame if ring buffer full — never blocks.
    /// INV-TB-2: never panics, never blocks.
    pub fn push(&self, frame: RealtimeFrame) {
        let last = self.last_poll_ms.load(Ordering::Relaxed);
        let now = Self::now_ms();
        if now.saturating_sub(last) > 2000 {
            return; // UI gone — skip FFT and push
        }
        if let Ok(mut prod) = self.producer.try_lock() {
            let _ = prod.push(frame);
        }
    }

    /// Non-blocking pop for Tauri command / HTTP endpoint.
    /// Updates last_poll_ms — resets the 2s auto-disable timer.
    /// Returns None if ring buffer empty.
    /// INV-TB-3: never panics, never blocks.
    pub fn pop(&self) -> Option<RealtimeFrame> {
        self.last_poll_ms.store(Self::now_ms(), Ordering::Relaxed);
        self.consumer.try_lock().ok()?.pop()
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

impl Default for RealtimeBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_roundtrip() {
        let bridge = RealtimeBridge::new();

        // Simulate UI poll to enable bridge
        bridge.pop();

        let frame = RealtimeFrame::silence(1000);
        bridge.push(frame);

        let popped = bridge.pop();
        assert!(popped.is_some(), "Should receive pushed frame");
        assert_eq!(popped.unwrap().position_ms, 1000);
    }

    #[test]
    fn push_drops_when_buffer_full() {
        let bridge = RealtimeBridge::new();

        // Enable bridge
        bridge.pop();

        // Fill buffer (capacity 4)
        for i in 0..6u64 {
            bridge.push(RealtimeFrame::silence(i));
        }

        // Should have at most 4 frames — extras dropped
        let mut count = 0;
        while bridge.pop().is_some() {
            count += 1;
        }
        assert!(count <= 4, "Ring buffer must not exceed capacity");
    }

    #[test]
    fn bypass_when_ui_silent() {
        let bridge = RealtimeBridge::new();

        // Never polled — last_poll_ms = 0
        // now - 0 > 2000ms → push is bypassed
        let frame = RealtimeFrame::silence(42);
        bridge.push(frame);

        // Nothing should be in buffer
        assert!(
            bridge.pop().is_none(),
            "Push should be bypassed when UI has not polled"
        );
    }

    #[test]
    fn pop_resets_timer() {
        let bridge = RealtimeBridge::new();
        let before = RealtimeBridge::now_ms();
        bridge.pop();
        let stored = bridge
            .last_poll_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(stored >= before, "pop() must update last_poll_ms");
    }
}
