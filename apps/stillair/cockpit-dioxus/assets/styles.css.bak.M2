/*
 * styles.css — Still Air Cockpit · Hardware Aesthetic v3
 * Authority: ui-agent-context.md v2.1
 *
 * ARCHITECTURE:
 *   .panel-screw-wrapper (position:relative, overflow:visible)
 *     .mfd-panel (overflow:hidden — clips content only)
 *     .screw × 4 (siblings — never clipped)
 *
 * MATERIAL LANGUAGE:
 *   Panel bezels: brushed metal linear-gradient(145deg)
 *   OLED cells:   true black #000000 with deep inset shadows
 *   Screws:       bright metallic, negative offset, sit ON bezel edge
 *   Titles:       0.875rem, letter-pressed / engraved look
 */

@import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@300;400;500;600;700&display=swap');

/* ═══════════════════════════════════════════════════════════════════════════════
   1. Design Tokens
   ═══════════════════════════════════════════════════════════════════════════════ */

:root {
    /* Backgrounds */
    --bg-base:             #111318;
    --bg-panel:            #15181f;
    --bg-elevated:         #1e2128;
    --bg-overlay:          #0d0f14;
    --bg-app:              #0d0f14;
    --bg-panel-inset:      #000000;   /* TRUE BLACK — OLED cells */
    --bg-meter:            #000000;
    --bg-finding-row:      #1e2128;
    --bg-track-active:     #1a3a38;

    /* Borders */
    --border-subtle:       #2a2d35;
    --border-active:       #3d4155;
    --border-panel:        #2a2d35;
    --border-inset:        #1a1a1a;
    --bracket-color:       #2a2d35;
    --screw-slot:          #090b10;

    /* Accent palette */
    --accent-session:      #FFD60A;
    --accent-insights:     #00d1ff;
    --accent-coach:        #4ade80;
    --accent-cyan:         #00d4c8;
    --accent-cyan-dim:     #007a74;
    --accent-cyan-glow:    rgba(0, 212, 200, 0.25);
    --accent-cyan-glow-sm: rgba(0, 212, 200, 0.12);
    --accent-amber:        #c8a832;
    --accent-amber-dim:    #7a6420;
    --accent-gold:         #d4a844;
    --accent-magenta:      #e040a0;

    /* Severity */
    --severity-high:       #ef4444;
    --severity-medium:     #f59e0b;
    --severity-low:        #fde047;
    --severity-info:       #6b7280;

    /* Hardware / Glass */
    --bg-glass:            #000000;
    --bg-cell:             #0a0c10;
    --text-title:          #c8c8c8;

    /* Text */
    --text-primary:        #e8eaf0;
    --text-secondary:      #8b8fa8;
    --text-muted:          #4a4d5e;
    --text-label:          #4a4a4a;
    --text-accent:         #00d4c8;
    --text-inverse:        #111318;

    /* State */
    --state-idle:          #4a4d5e;
    --state-active:        #4ade80;
    --state-running:       #00d1ff;
    --state-complete:      #FFD60A;
    --state-fault:         #ef4444;

    /* Buttons */
    --btn-master-bg:       #FFD60A;
    --btn-master-text:     #111318;
    --btn-neutral-bg:      #1e2128;
    --btn-neutral-text:    #8b8fa8;
    --btn-neutral-border:  #2a2d35;

    /* Typography */
    --font-mono: 'JetBrains Mono', 'Fira Code', monospace;
    --font-sans: 'Inter', system-ui, sans-serif;
    --text-xs:   0.625rem;
    --text-sm:   0.75rem;
    --text-base: 0.875rem;
    --text-lg:   1rem;
    --text-xl:   1.25rem;
    --text-2xl:  1.5rem;
    --tracking-wide:   0.05em;
    --tracking-wider:  0.1em;
    --tracking-widest: 0.15em;

    /* Spacing */
    --space-1: 0.25rem;
    --space-2: 0.5rem;
    --space-3: 0.75rem;
    --space-4: 1rem;
    --space-5: 1.25rem;
    --space-6: 1.5rem;
    --space-8: 2rem;

    /* Chassis dimensions */
    --chassis-transport-h: 56px;
    --chassis-header-h:    44px;
    --panel-inset:         12px;
    --bezel-radius:        6px;
    --panel-radius:        4px;
    --screw-size:          14px;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   2. Reset & Root
   ═══════════════════════════════════════════════════════════════════════════════ */

*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

html, body {
    height: 100%;
    overflow: visible;
    background: var(--bg-overlay);
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 14px;
    -webkit-font-smoothing: antialiased;
}

.app-shell,
#main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    overflow: visible;
    background:
        linear-gradient(160deg, rgba(255,255,255,0.025) 0%, transparent 40%, rgba(0,0,0,0.15) 100%),
        var(--bg-base);
}

/* ═══════════════════════════════════════════════════════════════════════════════
   3. Header / Mode Bar
   ═══════════════════════════════════════════════════════════════════════════════ */

.cockpit-header {
    height: var(--chassis-header-h);
    display: flex;
    align-items: center;
    padding: 0 var(--space-6);
    background: var(--bg-overlay);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    position: relative;
    z-index: 10;
    box-shadow: inset 0 -1px 0 rgba(255,255,255,0.03), 0 2px 8px rgba(0,0,0,0.4);
}

.cockpit-header::before,
.cockpit-header::after {
    content: '';
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: var(--screw-size);
    height: var(--screw-size);
    border-radius: 50%;
    background: radial-gradient(circle at 35% 35%, #3d4155, #1e2128 60%, #111318);
    border: 1px solid var(--border-subtle);
    pointer-events: none;
}
.cockpit-header::before { left: var(--space-3); }
.cockpit-header::after  { right: var(--space-3); }

.wordmark {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: 600;
    letter-spacing: var(--tracking-widest);
    color: var(--text-primary);
    text-transform: uppercase;
}

.mode-badge {
    margin-left: var(--space-4);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: var(--tracking-wider);
    color: var(--text-muted);
    text-transform: uppercase;
    padding: 2px 8px;
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    background: var(--bg-elevated);
}

.mode-badge.fm-running  { color: var(--state-running); border-color: var(--state-running); }
.mode-badge.fm-complete { color: var(--state-complete); border-color: var(--state-complete); }
.mode-badge.fm-fault    { color: var(--state-fault); border-color: var(--state-fault); animation: fault-pulse 1s ease-in-out infinite; }

@keyframes fault-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.6; } }

/* ═══════════════════════════════════════════════════════════════════════════════
   4. MFD Bay Grid
   ═══════════════════════════════════════════════════════════════════════════════ */

.mfd-bay {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: var(--panel-inset);
    padding: var(--panel-inset);
    min-height: 0;
    overflow: visible;
    isolation: isolate;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   5. Panel Screw Wrapper — positioning context for screws
   ═══════════════════════════════════════════════════════════════════════════════ */

.panel-screw-wrapper {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   6. MFD Panel — BRUSHED METAL CHASSIS
   ═══════════════════════════════════════════════════════════════════════════════ */

.mfd-panel {
    background:
        linear-gradient(
            145deg,
            #4a4d58 0%,
            #2e3138 40%,
            #1a1d24 100%
        );
    border: 2px solid #4a4d58;
    border-radius: var(--bezel-radius);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    position: relative;
    flex: 1;
    min-height: 0;
    min-width: 0;
    padding: 16px;
    box-shadow:
        inset 0 2px 0 rgba(255,255,255,0.25),
        inset 0 -2px 0 rgba(0,0,0,0.8),
        0 0 0 1px #000,
        0 8px 30px rgba(0,0,0,0.9);
}



/* ═══════════════════════════════════════════════════════════════════════════════
   7. Corner Screws — BRIGHT METALLIC, NEGATIVE OFFSET
   ═══════════════════════════════════════════════════════════════════════════════ */

.screw {
    position: absolute;
    width: var(--screw-size);
    height: var(--screw-size);
    border-radius: 50%;
    background:
        radial-gradient(
            circle at 30% 25%,
            #c8cdd8 0%,
            #8a8d98 25%,
            #4a4d58 60%,
            #0a0c14 100%
        );
    border: 1.5px solid rgba(255,255,255,0.40);
    box-shadow:
        inset 0 2px 2px rgba(255,255,255,0.50),
        inset 0 -2px 2px rgba(0,0,0,0.90),
        0 2px 4px rgba(0,0,0,1.0),
        0 0 8px rgba(0,0,0,0.70);
    z-index: 20;
    pointer-events: none;
}

/* Etched slot line across screw face */
.screw::after {
    content: "";
    position: absolute;
    top: 50%;
    left: 15%;
    width: 70%;
    height: 1.5px;
    background: var(--screw-slot);
    transform: translateY(-50%) rotate(45deg);
    border-radius: 1px;
    pointer-events: none;
}

/* NEGATIVE OFFSETS — screws sit ON the bezel edge, partially outside */
.screw-tl { top: 12px;    left: 12px;  }
.screw-tr { top: 12px;    right: 12px; }
.screw-bl { bottom: 12px; left: 12px;  }
.screw-br { bottom: 12px; right: 12px; }

/* ═══════════════════════════════════════════════════════════════════════════════
   8. Panel Title — ENGRAVED LOOK
   ═══════════════════════════════════════════════════════════════════════════════ */

/* Trick to override inline styles without touching .rs files or !important */
@keyframes m1-title-override {
    to {
        color: var(--text-title);
        font-weight: 400;
        border-bottom: none;
        padding: 4px 12px 8px;
    }
}

.panel-title {
    font-family: var(--font-mono);
    font-size: 0.95rem;
    font-weight: 400;
    letter-spacing: 0.2em;
    text-transform: uppercase;
    padding: 4px 12px 8px;
    flex-shrink: 0;
    position: relative;
    color: var(--text-title);
    background: transparent;
    text-shadow: 0 1px 0 rgba(0,0,0,0.6);
    /* Apply override to beat inline styles */
    animation: m1-title-override 0.001s forwards;
}

/* Accent underline glow */
.panel-title::after {
    content: '';
    display: block;
    position: absolute;
    bottom: 0;
    left: 12px;
    right: 12px;
    height: 1px;
    background: var(--accent-insights, #00d1ff);
    opacity: 0.6;
    box-shadow: 0 0 6px var(--accent-insights, #00d1ff);
    pointer-events: none;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   9. Panel Body
   ═══════════════════════════════════════════════════════════════════════════════ */

.panel-body {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
    scrollbar-color: var(--border-active) transparent;
    padding: 16px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: #000000;
    box-shadow:
        inset 0 4px 20px rgba(0,0,0,0.95),
        inset 0 0 0 1px rgba(255,255,255,0.03);
    border-radius: 4px;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   10. OLED Screen Cells — TRUE BLACK with deep inset
   ═══════════════════════════════════════════════════════════════════════════════ */

.oled-screen {
    background: #000000;
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 3px;
    box-shadow:
        inset 0 2px 12px rgba(0,0,0,0.95),
        inset 0 0 30px rgba(0,0,0,0.7),
        inset 0 0 0 1px rgba(255,255,255,0.03);
    position: relative;
}

/* Low-power mode override */
[data-low-power="true"] * {
    box-shadow: none;
    filter: none;
    text-shadow: none;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   11. Insights Panel Layout
   ═══════════════════════════════════════════════════════════════════════════════ */

.insights-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    min-height: 0;
    overflow: hidden;
    background: var(--bg-glass);
    box-shadow:
        inset 0 4px 16px rgba(0,0,0,1),
        inset 0 0 40px rgba(0,0,0,0.7),
        inset 0 0 0 1px rgba(255,255,255,0.04),
        0 1px 0 rgba(255,255,255,0.06);
    border-radius: 4px;
}

.insights-spectrum-cell {
    flex: 0 0 55%;
    min-height: 0;
    overflow: hidden;
    position: relative;
    background: var(--bg-cell);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 10px;
    box-shadow: none;
    padding: 10px 14px;
}

.insights-bottom-row {
    flex: 1;
    display: flex;
    flex-wrap: nowrap;
    gap: 12px;
    min-height: 0;
    overflow: hidden;
}

.insights-scope-cell {
    flex: 0 0 45%;
    min-height: 0;
    overflow: hidden;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-cell);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 10px;
    box-shadow: none;
    padding: 10px 14px;
}

.insights-meters-cell {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    position: relative;
    background: var(--bg-cell);
    border: 1px solid rgba(255,255,255,0.06);
    border-radius: 10px;
    box-shadow: none;
    padding: 10px 14px;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   12. Spectrum
   ═══════════════════════════════════════════════════════════════════════════════ */

.spectrum-wrap { width: 100%; height: 100%; display: flex; flex-direction: column; }
.spectrum-svg  { flex: 1; width: 100%; min-height: 0; display: block; }

.spectrum-footer {
    display: flex;
    justify-content: space-between;
    padding: 2px 8px;
    background: rgba(0,0,0,0.4);
    font-family: var(--font-mono);
    font-size: 0.5rem;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
}

.spectrum-readout      { color: var(--accent-insights); }
.spectrum-readout-peak { color: var(--accent-amber); }

/* ═══════════════════════════════════════════════════════════════════════════════
   13. StereoScope
   ═══════════════════════════════════════════════════════════════════════════════ */

.scope-wrap {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
}

.scope-wrap svg {
    width: min(100%, 120px);
    height: min(100%, 120px);
    filter: drop-shadow(0 0 4px rgba(0,209,255,0.3));
}

/* ═══════════════════════════════════════════════════════════════════════════════
   14. VU Panel
   ═══════════════════════════════════════════════════════════════════════════════ */

.vu-panel { width: 100%; height: 100%; display: flex; flex-direction: column; padding: 4px; }

.vu-meters-row {
    display: flex;
    gap: 4px;
    align-items: flex-end;
    flex: 1;
    min-height: 0;
}

.vu-scale {
    display: flex;
    flex-direction: column-reverse;
    justify-content: space-between;
    align-items: flex-end;
    height: 100%;
    padding: 2px 0;
}

.vu-scale-label {
    font-family: var(--font-mono);
    font-size: 0.38rem;
    color: var(--text-muted);
    height: 6px;
    display: flex;
    align-items: center;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
}

.vu-channel-wrap { display: flex; flex-direction: column; align-items: center; gap: 2px; height: 100%; }

.vu-bar {
    display: flex;
    flex-direction: column-reverse;
    gap: 2px;
    width: 18px;
    flex-shrink: 0;
    height: 100%;
}

.vu-seg { height: 4px; border-radius: 1px; flex-shrink: 0; }

.vu-ch-label {
    font-family: var(--font-mono);
    font-size: 0.42rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent-insights);
    flex-shrink: 0;
    margin-bottom: 2px;
}

.vu-peak-label { color: var(--accent-amber); }

.vu-readout-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: space-around;
    padding-left: 4px;
}

.vu-readout-row  { display: flex; flex-direction: column; }
.vu-readout-label {
    font-family: var(--font-mono);
    font-size: 0.4rem;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    color: var(--text-muted);
    line-height: 1;
}

.vu-readout-value {
    font-family: var(--font-mono);
    font-size: 1.1rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
    letter-spacing: -0.02em;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   15. Coach Panel — Finding Rows
   ═══════════════════════════════════════════════════════════════════════════════ */

.finding-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px;
    margin: 8px 12px;
    border-radius: 10px;
    background: #0a0c10;
    border: 1px solid rgba(255,255,255,0.06);
    box-shadow: none;
}

.finding-row-header { display: flex; align-items: center; justify-content: space-between; gap: 6px; }

.finding-row-label {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
    color: var(--text-primary);
}

.finding-row-description {
    font-family: var(--font-mono);
    font-size: 0.58rem;
    color: var(--text-secondary);
    line-height: 1.4;
}

.finding-bar-label { display: flex; align-items: center; gap: 5px; margin-top: 5px; }
.finding-bar-track { flex: 1; height: 5px; background: rgba(255,255,255,0.07); border-radius: 3px; overflow: hidden; }
.finding-bar-fill  { height: 100%; background: var(--accent-cyan); border-radius: 3px; box-shadow: 0 0 6px rgba(0,209,255,0.35); }
.finding-bar-pct   { font-family: var(--font-mono); font-size: 0.5rem; color: var(--text-secondary); min-width: 28px; text-align: right; font-variant-numeric: tabular-nums; }

.severity-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
.severity-dot.high   { background: var(--severity-high);   box-shadow: 0 0 5px var(--severity-high); }
.severity-dot.medium { background: var(--severity-medium); box-shadow: 0 0 5px var(--severity-medium); }
.severity-dot.low    { background: var(--severity-low);    box-shadow: 0 0 5px var(--severity-low); }
.severity-dot.info   { background: var(--accent-cyan);     box-shadow: 0 0 4px var(--accent-cyan-glow); }

/* ═══════════════════════════════════════════════════════════════════════════════
   16. Coach Actions (YES/NO)
   ═══════════════════════════════════════════════════════════════════════════════ */

.coach-actions { display: flex; gap: 8px; padding: 12px 12px 16px; margin-top: 4px; }

.btn-yes {
    flex: 1;
    background: var(--accent-cyan);
    color: var(--bg-overlay);
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    padding: 8px 0;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    text-transform: uppercase;
    box-shadow: 0 0 12px rgba(0,209,255,0.3);
    pointer-events: all;
    position: relative;
    z-index: 1;
}

.btn-yes:hover { box-shadow: 0 0 20px rgba(0,209,255,0.5); }

.btn-no {
    flex: 1;
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.12em;
    padding: 8px 0;
    border: 1px solid var(--border-active);
    border-radius: 4px;
    cursor: pointer;
    text-transform: uppercase;
    pointer-events: all;
    position: relative;
    z-index: 1;
}

.btn-no:hover { border-color: var(--text-secondary); }

/* ═══════════════════════════════════════════════════════════════════════════════
   17. Session Brackets
   ═══════════════════════════════════════════════════════════════════════════════ */

.session-bracket-bl {
    position: absolute;
    bottom: 12px; left: 12px;
    width: 20px; height: 20px;
    border-left: 2px solid var(--border-active);
    border-bottom: 2px solid var(--border-active);
    pointer-events: none;
    opacity: 0.5;
}

.session-bracket-br {
    position: absolute;
    bottom: 12px; right: 12px;
    width: 20px; height: 20px;
    border-right: 2px solid var(--border-active);
    border-bottom: 2px solid var(--border-active);
    pointer-events: none;
    opacity: 0.5;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   18. Transport Bar
   ═══════════════════════════════════════════════════════════════════════════════ */

.transport-bar {
    height: var(--chassis-transport-h);
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: 0 var(--space-8);
    background: var(--bg-overlay);
    border-top: 1px solid rgba(255,255,255,0.06);
    flex-shrink: 0;
    position: relative;
    z-index: 10;
    pointer-events: auto;
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.05), 0 -2px 12px rgba(0,0,0,0.4);
}

.transport-bar::before,
.transport-bar::after {
    content: '';
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: var(--screw-size);
    height: var(--screw-size);
    border-radius: 50%;
    background: radial-gradient(circle at 35% 35%, #3d4155, #1e2128 60%, #111318);
    border: 1px solid var(--border-subtle);
    pointer-events: none;
}
.transport-bar::before { left: var(--space-3); }
.transport-bar::after  { right: var(--space-3); }

.transport-time {
    font-family: var(--font-mono);
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--accent-insights);
    text-shadow: 0 0 12px rgba(0,209,255,0.5);
    font-variant-numeric: tabular-nums;
    min-width: 72px;
    user-select: none;
}

.transport-duration {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    min-width: 52px;
    text-align: right;
    user-select: none;
}

.transport-divider { width: 1px; height: 24px; background: var(--border-active); flex-shrink: 0; }

.transport-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 1;
    justify-content: center;
    pointer-events: auto;
}

.transport-mode {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: var(--tracking-wider);
    color: var(--text-muted);
    text-transform: uppercase;
    padding: 2px var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: 3px;
    background: var(--bg-elevated);
    user-select: none;
    white-space: nowrap;
    flex-shrink: 0;
}

.transport-skip {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: var(--tracking-wider);
    text-transform: uppercase;
    padding: var(--space-1) var(--space-3);
    background: var(--btn-neutral-bg);
    color: var(--btn-neutral-text);
    border: 1px solid var(--btn-neutral-border);
    border-radius: 3px;
    cursor: pointer;
    position: relative;
    z-index: 1;
    pointer-events: auto;
    white-space: nowrap;
}

.transport-skip:hover:not(:disabled) { color: var(--text-primary); border-color: var(--border-active); }
.transport-skip:disabled { opacity: 0.25; cursor: not-allowed; }

.transport-play {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    font-weight: 700;
    letter-spacing: var(--tracking-wider);
    text-transform: uppercase;
    padding: var(--space-2) var(--space-5);
    background: var(--btn-master-bg);
    color: var(--btn-master-text);
    border: none;
    border-radius: 3px;
    cursor: pointer;
    position: relative;
    z-index: 1;
    pointer-events: auto;
    box-shadow: 0 0 14px rgba(255,214,10,0.3);
}

.transport-play:hover:not(:disabled) { box-shadow: 0 0 22px rgba(255,214,10,0.5); }

.transport-play.is-playing {
    background: var(--btn-neutral-bg);
    color: var(--accent-insights);
    border: 1px solid rgba(0,209,255,0.4);
    box-shadow: 0 0 12px rgba(0,209,255,0.25);
}

.transport-play:disabled { opacity: 0.25; cursor: not-allowed; }

.transport-scrub {
    flex: 1;
    max-width: 240px;
    height: 28px;
    position: relative;
    cursor: pointer;
    display: flex;
    align-items: center;
}

.transport-scrub-track {
    position: absolute;
    left: 0; right: 0;
    top: 50%;
    transform: translateY(-50%);
    height: 3px;
    background: var(--border-active);
    border-radius: 2px;
    overflow: hidden;
}

.transport-scrub-fill {
    height: 100%;
    background: var(--accent-insights);
    border-radius: 2px;
    box-shadow: 0 0 6px rgba(0,209,255,0.6);
    transition: width 0.15s linear;
}

.transport-scrub-head {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent-insights);
    box-shadow: 0 0 8px rgba(0,209,255,0.8);
    pointer-events: none;
}

.transport-scrub-label {
    position: absolute;
    bottom: -2px;
    left: 0; right: 0;
    text-align: center;
    font-family: var(--font-mono);
    font-size: 0.5rem;
    color: var(--text-muted);
    letter-spacing: var(--tracking-widest);
    text-transform: uppercase;
    pointer-events: none;
}

/* ═══════════════════════════════════════════════════════════════════════════════
   19. Drop Zone
   ═══════════════════════════════════════════════════════════════════════════════ */

.drop-zone {
    border: 2px dashed var(--border-active);
    border-radius: var(--bezel-radius);
    padding: 32px 16px;
    text-align: center;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    cursor: pointer;
    margin: 0;
}

.drop-zone:hover { border-color: var(--accent-session); color: var(--accent-session); }
.drop-zone .drop-icon  { font-size: 2rem; opacity: 0.25; display: block; margin-bottom: var(--space-3); }
.drop-zone .drop-types { font-size: var(--text-xs); margin-top: var(--space-2); opacity: 0.5; }

/* ═══════════════════════════════════════════════════════════════════════════════
   20. Preset Menu
   ═══════════════════════════════════════════════════════════════════════════════ */

.preset-menu { display: flex; flex-direction: column; gap: var(--space-1); padding: var(--space-4) 0; }

.preset-btn {
    background: var(--bg-elevated);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--panel-radius);
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    cursor: pointer;
    text-align: left;
}

.preset-btn:hover,
.preset-btn.selected { border-color: var(--accent-session); color: var(--accent-session); }

/* ═══════════════════════════════════════════════════════════════════════════════
   21. Mastering Progress
   ═══════════════════════════════════════════════════════════════════════════════ */

.mastering-progress { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; padding: var(--space-8) var(--space-6); text-align: center; }

.mastering-label {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: var(--tracking-widest);
    text-transform: uppercase;
    color: var(--state-running);
    text-shadow: 0 0 10px rgba(0,209,255,0.4);
    margin-bottom: var(--space-6);
}

.progress-track { width: 100%; height: 2px; background: var(--border-subtle); border-radius: 2px; overflow: hidden; }

.progress-fill {
    height: 100%;
    background: var(--state-running);
    border-radius: 2px;
    box-shadow: 0 0 8px var(--state-running);
    animation: progress-sweep 2s ease-in-out infinite;
}

@keyframes progress-sweep { 0% { width: 20%; } 50% { width: 80%; } 100% { width: 20%; } }

.mastering-sub { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--text-muted); margin-top: var(--space-4); letter-spacing: var(--tracking-wider); text-transform: uppercase; }

/* ═══════════════════════════════════════════════════════════════════════════════
   22. Golden Blob Badge
   ═══════════════════════════════════════════════════════════════════════════════ */

.blob-badge { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-3) 0; border-bottom: 1px solid var(--border-subtle); }

.blob-indicator { width: 8px; height: 8px; border-radius: 50%; background: var(--state-complete); box-shadow: 0 0 8px var(--state-complete); flex-shrink: 0; animation: blob-glow 2s ease-in-out infinite; }

@keyframes blob-glow { 0%, 100% { box-shadow: 0 0 6px var(--state-complete); } 50% { box-shadow: 0 0 14px var(--state-complete); } }

.blob-text { font-family: var(--font-mono); font-size: var(--text-xs); letter-spacing: var(--tracking-widest); text-transform: uppercase; color: var(--text-secondary); }

/* ═══════════════════════════════════════════════════════════════════════════════
   23. File Info
   ═══════════════════════════════════════════════════════════════════════════════ */

.file-info { padding: var(--space-4) 0; border-bottom: 1px solid var(--border-subtle); }
.file-info-name { font-family: var(--font-mono); font-size: var(--text-base); font-weight: 500; color: var(--text-primary); word-break: break-all; }
.file-info-meta { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--text-muted); margin-top: var(--space-1); letter-spacing: var(--tracking-wider); text-transform: uppercase; }

/* ═══════════════════════════════════════════════════════════════════════════════
   24. Metric Rows
   ═══════════════════════════════════════════════════════════════════════════════ */

.metric-row { display: flex; justify-content: space-between; align-items: baseline; padding: var(--space-2) 0; border-bottom: 1px solid rgba(255,255,255,0.025); }
.metric-label { font-family: var(--font-mono); font-size: var(--text-xs); letter-spacing: var(--tracking-wider); text-transform: uppercase; color: var(--text-muted); }
.metric-value { font-family: var(--font-mono); font-size: var(--text-base); font-weight: 500; color: var(--text-primary); font-variant-numeric: tabular-nums; }
.metric-value.ok   { color: var(--state-active); }
.metric-value.warn { color: var(--severity-medium); }
.metric-value.err  { color: var(--severity-high); }
.lufs-readout { font-family: var(--font-mono); font-size: var(--text-2xl); font-weight: 700; color: var(--accent-insights); font-variant-numeric: tabular-nums; text-shadow: 0 0 16px rgba(0,209,255,0.4); }

/* ═══════════════════════════════════════════════════════════════════════════════
   25. Compliance
   ═══════════════════════════════════════════════════════════════════════════════ */

.compliance-grid { display: grid; grid-template-columns: 1fr 1fr; gap: var(--space-1); padding: var(--space-2) 0; }
.compliance-row  { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-1) 0; }
.compliance-dot  { width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0; }
.compliance-dot.ok  { background: var(--state-active); box-shadow: 0 0 5px var(--state-active); }
.compliance-dot.err { background: var(--state-fault);  box-shadow: 0 0 5px var(--state-fault); }
.compliance-name { font-family: var(--font-mono); font-size: var(--text-xs); color: var(--text-secondary); }

/* ═══════════════════════════════════════════════════════════════════════════════
   26. Export Controls
   ═══════════════════════════════════════════════════════════════════════════════ */

.export-format-btn { font-family: var(--font-mono); font-size: var(--text-xs); letter-spacing: var(--tracking-widest); text-transform: uppercase; padding: var(--space-1) var(--space-3); border-radius: 3px; cursor: pointer; border: 1px solid var(--border-subtle); background: var(--bg-elevated); color: var(--text-muted); }
.export-format-btn.active { background: rgba(0,209,255,0.1); color: var(--accent-insights); border-color: rgba(0,209,255,0.4); }

/* ═══════════════════════════════════════════════════════════════════════════════
   27. Transport Action Buttons
   ═══════════════════════════════════════════════════════════════════════════════ */

.transport-btn { font-family: var(--font-mono); font-size: var(--text-xs); font-weight: 600; letter-spacing: var(--tracking-wider); text-transform: uppercase; padding: var(--space-2) var(--space-4); border-radius: 3px; cursor: pointer; border: none; white-space: nowrap; position: relative; z-index: 1; pointer-events: auto; }
.transport-btn.neutral { background: var(--btn-neutral-bg); color: var(--btn-neutral-text); border: 1px solid var(--btn-neutral-border); }
.transport-btn.master  { background: var(--btn-master-bg); color: var(--btn-master-text); font-weight: 700; box-shadow: 0 0 12px rgba(255,214,10,0.25); }
.transport-btn:disabled { opacity: 0.3; cursor: not-allowed; }

/* ═══════════════════════════════════════════════════════════════════════════════
   28. Fault Overlay
   ═══════════════════════════════════════════════════════════════════════════════ */

.fault-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; z-index: 100; }
.fault-card    { background: var(--bg-elevated); border: 1px solid var(--severity-high); border-radius: var(--bezel-radius); padding: var(--space-8); text-align: center; max-width: 400px; }
.fault-code    { font-family: var(--font-mono); font-size: var(--text-xl); font-weight: 700; color: var(--severity-high); letter-spacing: var(--tracking-widest); text-transform: uppercase; margin-bottom: var(--space-3); }
.fault-message { font-family: var(--font-mono); font-size: var(--text-sm); color: var(--text-secondary); line-height: 1.6; margin-bottom: var(--space-6); }

/* ═══════════════════════════════════════════════════════════════════════════════
   29. Mastered View
   ═══════════════════════════════════════════════════════════════════════════════ */

.mastered-view { position: fixed; inset: 0; background: var(--bg-app); z-index: 50; display: grid; grid-template-columns: 200px 1fr; grid-template-rows: 1fr auto; }
.mastered-sidebar { background: var(--bg-panel); border-right: 1px solid var(--border-panel); padding: 24px 16px; display: flex; flex-direction: column; gap: 8px; grid-row: 1 / span 2; }
.mastered-nav-item { font-family: var(--font-mono); font-size: 0.625rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-secondary); padding: 6px 8px; border-radius: 3px; cursor: pointer; }
.mastered-nav-item:hover, .mastered-nav-item.active { color: var(--accent-cyan); background: var(--accent-cyan-glow-sm); }
.mastered-main { padding: 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
.mastered-view-modes { display: flex; gap: 8px; }
.view-mode-btn { font-family: var(--font-mono); font-size: 0.5rem; letter-spacing: 0.15em; text-transform: uppercase; padding: 4px 12px; border: 1px solid var(--border-panel); border-radius: 3px; background: transparent; color: var(--text-secondary); cursor: pointer; }
.view-mode-btn.active { border-color: var(--accent-cyan); color: var(--accent-cyan); }
.waveform-container { background: var(--bg-panel-inset); border: 1px solid var(--border-inset); border-radius: 3px; padding: 8px; }
.waveform-label { font-family: var(--font-mono); font-size: 0.5rem; letter-spacing: 0.15em; text-transform: uppercase; color: var(--text-muted); margin-bottom: 4px; }
.waveform-svg { width: 100%; height: 120px; display: block; }
.quality-gate-panel { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.quality-gate-value { font-family: var(--font-mono); font-size: 1.75rem; font-weight: 700; color: var(--accent-cyan); font-variant-numeric: tabular-nums; }
.compliance-panel { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px; }
.compliance-led-dot { width: 10px; height: 10px; border-radius: 50%; }
.compliance-led-dot.active   { background: var(--accent-cyan); box-shadow: 0 0 8px var(--accent-cyan-glow); }
.compliance-led-dot.inactive { background: var(--accent-cyan-dim); opacity: 0.3; }
.compliance-led-label { font-family: var(--font-mono); font-size: 0.45rem; letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-muted); text-align: center; }
.mastered-export-bar { grid-column: 2; display: flex; gap: 8px; padding: 12px 24px; background: var(--bg-panel); border-top: 1px solid var(--border-panel); align-items: center; flex-wrap: wrap; }

/* ═══════════════════════════════════════════════════════════════════════════════
   30. Utilities
   ═══════════════════════════════════════════════════════════════════════════════ */

.hidden { display: none; }
.awaiting { display: flex; align-items: center; justify-content: center; height: 100%; font-family: var(--font-mono); font-size: var(--text-xs); letter-spacing: var(--tracking-widest); text-transform: uppercase; color: var(--text-muted); padding: var(--space-8); text-align: center; }
@keyframes led-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
.led-pulse { animation: led-pulse 1.8s ease-in-out infinite; }

/* Section labels */
.section-label { font-family: var(--font-mono); font-size: var(--text-xs); letter-spacing: var(--tracking-wider); text-transform: uppercase; color: var(--text-muted); padding: var(--space-3) 0 var(--space-1); border-top: 1px solid var(--border-subtle); }
