---
name: dev-testing-toolkit
description: >-
  How to run, debug, and verify Rust/cargo daemons and test suites in this repo
  without blocking the terminal or losing track of what's actually been proven.
  Covers: backgrounding long-running processes (daemons, servers) correctly,
  diagnostic eprintln/tracing trap conventions and feature-flagging them out of
  hot paths, reading log output without a browser console, and the difference
  between "compiles" and "actually ran and passed." Use this whenever starting
  m0d/xaak/any cargo run daemon for manual testing, adding temporary debug
  output, running curl-based integration checks, or when a terminal command
  seems to hang/not return control.
---

# Dev Testing Toolkit

Practical rules for manually testing this codebase (daemons, DSP pipelines, telemetry) without wasting time on blocked terminals or unverified claims.

## 1. Never run a daemon in the foreground when you need to do anything after it

`cargo run --bin m0d` (or any long-running server) blocks the shell until killed. If you need to send it requests afterward, you MUST background it — but background it **correctly**, or it silently doesn't background and your terminal hangs forever waiting for a process that never exits.

**Correct pattern — every command on its own line, never inside a single multi-line heredoc/script block:**

```bash
killall m0d 2>/dev/null; sleep 1; echo "killed"
```
```bash
RUST_LOG=info,m0d=debug cargo run --bin m0d > /tmp/test.log 2>&1 &
```
```bash
echo "shell is free"
```

The third command is your proof the backgrounding worked. If `echo "shell is free"` doesn't print immediately, the previous command did NOT background — stop, do not run anything else, and report "terminal did not return control" rather than trying something else blind.

**Why this breaks:** putting `cargo run ... &` inside a larger heredoc, a `cat << 'EOF' > script.sh` block, or chaining many commands with `&&` on one logical line increases the chance the `&` gets parsed as part of something else, or that an earlier command in the chain never returns so the `&` is never reached. One command per turn removes the ambiguity.

**Wait for readiness, don't guess a sleep duration:**
```bash
for i in {1..30}; do if grep -q "pre-warm done" /tmp/test.log; then echo "ready"; break; fi; sleep 1; done
```
Poll for a known log line instead of `sleep 10` and hoping.

## 2. If a terminal seems stuck: stop, don't improvise

If a command should have returned and didn't (no new prompt, no output), the most common cause is a foreground process that should have been backgrounded. The WRONG response is to start exploring the codebase looking for an unrelated bug ("maybe it's something about the LUFS calculation") — that is a panic response, not a diagnosis.

**The right response:**
1. Say explicitly: "the terminal isn't returning control, this looks like a blocked foreground process."
2. If you have access to a separate terminal/session, kill the suspected process from there (`killall <binary>`) and confirm with `ps aux | grep <binary>`.
3. Re-run using the one-command-per-turn background pattern in §1.
4. Do not touch unrelated source files while diagnosing this. The bug is almost never in application code when the symptom is "the shell didn't come back."

## 3. Diagnostic traps: add them freely, but commit them deliberately

It's normal and encouraged to add temporary `eprintln!`/`tracing::debug!` traps while chasing a bug — this codebase has used them heavily (`[RAW-TELEM]`, `[ACTOR-IDLE]`, `[PRESET-DEBUG]`, etc.) to get hard evidence instead of guessing.

**Two categories, two different fates:**

- **One-shot / state-transition traps** (fire rarely: on mount, on error, on a specific decision branch) — fine to leave in permanently. Near-zero runtime cost, useful as standing production diagnostics. No feature flag needed.
- **Hot-path traps** (fire every frame, every loop iteration, every audio callback) — these have a real cost (stderr writes, string formatting, allocations) even when "off" unless they're compiled out. If a trap fires more than ~once per second in steady state, gate it behind a Cargo feature flag before committing:

```toml
[features]
debug-telem = []
```
```rust
#[cfg(feature = "debug-telem")]
eprintln!("[RAW-TELEM] ...");
```

Verify BOTH states compile clean before committing:
```bash
cargo check -p <crate>                      # feature off — confirm the trap fully disappears
cargo check -p <crate> --features debug-telem  # feature on — confirm it still compiles with it active
```

**Before committing any fix that used temporary traps to prove itself:** decide explicitly whether each trap is now removed, left permanently (because it's cheap/useful), or feature-flagged. Don't leave hot-path `eprintln!` calls in the default build path "to clean up later" — they don't get cleaned up later.

## 4. Reading output: prefer the terminal log over a browser console

If a fix touches frontend code (Dioxus/WASM) and you need to see `console.log` output, the browser DevTools console in this environment chokes on high-frequency output (becomes unresponsive, truncates messages, "N console messages are not shown"). Don't fight it.

**Bridge pattern already in this codebase:** a Tauri command (`frontend_log`) that does `eprintln!` server-side, called fire-and-forget from the frontend trap sites via `spawn_local`. This routes WASM-side diagnostics into the same `grep`-able terminal log as the backend daemon. If you're adding new frontend traps and the existing bridge command covers your case, use it instead of `web_sys::console::log_1` alone.

If you genuinely need to read the live browser console (e.g. confirming a UI behavior that has no backend-observable signal), say so explicitly and ask the user to paste it — don't try browser automation workarounds (view-source tricks, scripted DOM scraping) to get around lacking direct access. That wastes turns and usually doesn't work.

## 5. "Compiles" is not "works" — say which one you have

`cargo check` proves the code is syntactically and type-valid. It proves nothing about runtime behavior. When a fix can't be runtime-tested immediately (e.g. blocked by an unrelated broken binary sharing a `required-features` gate, or a feature flag not exercised by the default test run), say so explicitly in the verification summary and in any commit message:

- "Compile-checked but not run" ≠ "verified"
- If you only have compile evidence, the commit message must say that plainly, not imply full verification.

When you DO get runtime evidence, prefer a hard log assertion over inference from indirect signals. Example: to prove a config value resolved correctly, don't infer it from the final LUFS reading (which depends on input content too) — print the resolved value directly at the point it's read, confirm it in the log, then remove the trap.

## 6. Quick decision guide

| Situation | Tool |
|---|---|
| Need to see what a long-running daemon is doing over time | `cargo run ... > /tmp/x.log 2>&1 &` + `grep`/`tail -f` on the log file |
| Need to send one-off requests to a running daemon | `curl` against its HTTP endpoints, daemon already backgrounded per §1 |
| Need to confirm a specific value was computed/resolved correctly | A targeted, removable trap printing that exact value — not inference from a downstream side effect |
| Need to verify a fix didn't break anything else | Full test suite (`just ci` or equivalent), not just the one test you were focused on |
| A test passes but you're not sure if it actually ran (e.g. wrong feature flag, wrong binary target) | Check the actual test *count* in the output, not just exit code — "0 passed" with exit 0 means nothing ran |
| Frontend behavior needs visual/console confirmation and no backend-observable signal exists | Ask the user directly for a screenshot/console paste — don't improvise workarounds |

---
