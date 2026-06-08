# Creator-OS: Constitutional Agent Architecture (v3.1)

**Status:** 🔒 SPECIFICATION (Draft)
**Layer:** Agent Layer (M0 Daemon Subsystems)
**Purpose:** To define the strict boundaries, responsibilities, and communication protocols of the micro-agent subsystems orchestrating Creator-OS.

---

## 1. Architectural Philosophy

The Agent Layer is the operational heart of Creator-OS. It adheres strictly to the **Constitutional Separation of Concerns**. 

In legacy systems (v2.0), the daemon (`m0-daemon`) was a monolith that read files, executed DSP, wrote state, and generated reports simultaneously. 

In **v3.1**, the system is divided into highly specialized **Agents**.
*   **Single Responsibility:** Every agent has exactly one job.
*   **No Super-Privileges:** No agent can bypass the rules of another. (e.g., The DSP engine cannot write to the project file directly; it must ask the Schema Agent).
*   **Determinism:** Agents do not use heuristics or machine learning. They operate on strict rules and schemas.

---

## 2. The Constitutional Agents

### 2.1 Schema Agent
**Role:** The Guardian of State.
*   **Responsibility:** The *only* entity in the entire operating system permitted to write, mutate, or update the Project State (manifest).
*   **Mechanism:** It receives mutation requests (Intents) from other agents or the UI. It validates these requests strictly against predefined JSON Schemas. If a request is invalid, it is rejected.
*   **Constraint:** Does not know about audio. Does not run DSP. Only knows Data structures.

### 2.2 Golden Blob Agent
**Role:** The Custodian of Lineage & History.
*   **Responsibility:** Manages the immutable binary artifacts produced by the DSP engines.
*   **Mechanism:** When the DSP finishes, this agent hashes the raw PCM data (BLAKE3), stores it securely, and maintains the **A/B/C Tree** (the branching history of user decisions).
*   **Constraint:** Never alters audio. It only tracks, hashes, and retrieves exact versions. It guarantees bit-for-bit reproducibility.

### 2.3 OpenClaw Workflow Agent
**Role:** The Execution Distributor (`distributor.rs` / `operator.rs`).
*   **Responsibility:** The engine driver. It reads the current state and orchestrates the deterministic pipeline execution.
*   **Mechanism:** It hands data over to `sp314-dsp` (Engine Layer), tells it exactly what parameters to use, waits for the result, and then routes the result to the Golden Blob Agent.
*   **Constraint:** It does not decide *what* to do (that is the user's/schema's job), it only dictates *how* to execute it safely, efficiently, and concurrently.

### 2.4 Coach Agent
**Role:** The Rule-Based Analyst.
*   **Responsibility:** Translates raw DSP telemetry and metrics into human-readable, deterministic findings.
*   **Mechanism:** It reads the EBU R128 metrics, True Peak data, and NMF Mask values. It applies strict "If-This-Then-That" thresholds to generate deterministic findings (e.g., "Vocals are masked by 12% in the 2kHz range").
*   **Constraint:** It does NOT use LLMs. It generates hard facts. These facts are later fed to the Persona Layer.

### 2.5 Export Agent
**Role:** The Final Renderer.
*   **Responsibility:** Handles the packaging of the final master and the generation of visual proofs.
*   **Mechanism:** It takes the finalized Golden Blob and the cryptographic timeline, rendering them into FLAC/WAV files, and generating the native PDF and PNG Forensic Certificates.
*   **Constraint:** It only runs at the very end of the lifecycle. It cannot alter the DSP or the state.

### 2.6 Persona Context Agent
**Role:** The Memory Manager for Personas.
*   **Responsibility:** Maintains the context window, active variables, and memory states for the Schema-Driven Personas (like JINI).
*   **Mechanism:** It ensures that when JINI "speaks" or makes an action, she has the exact, correct context of what the user is currently looking at in the avionics UI.
*   **Constraint:** It does not run LLM inference. It merely organizes the strict schema packages that will be sent to the Persona runtime.

---

## 3. Inter-Agent Communication (The Firewall)

Agents do not call each other's functions directly (no spaghetti code). 
They communicate via a **Message Bus / Intent System**.

1.  **UI Intent:** User clicks "Master".
2.  **OpenClaw Agent:** Sees intent, locks the pipeline, dispatches job to `sp314-dsp`.
3.  **Golden Blob Agent:** Catches output, hashes it, saves the binary.
4.  **Schema Agent:** Receives the hash and updates the Project State safely.
5.  **Coach Agent:** Reads the new state, generates telemetry findings.
6.  **Persona Context Agent:** Updates JINI's memory that a new master was created.

This architecture ensures that Creator-OS remains unbreakable, fully auditable, and truly "avionics-grade."
