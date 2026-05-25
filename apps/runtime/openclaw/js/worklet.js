import init, { OpenClawEngine } from './e11.js';

class OpenClawWorklet extends AudioWorkletProcessor {
    constructor() {
        super();
        this.engine = null;
        this.ready = false;

        this.port.onmessage = async (event) => {
            const { type, data } = event.data;

            if (type === 'INIT') {
                await init(data.wasmBytes);
                this.engine = new OpenClawEngine(
                    data.topologyJson,
                    data.blockSize,
                    sampleRate
                );
                this.ready = true;
                this.port.postMessage({ type: 'READY' });
            }

            if (type === 'LOAD_TAB') {
                if (this.engine) {
                    this.engine.load_time_aware_behaviour(data.tabJson);
                    this.tabLoaded = true;
                    this.port.postMessage({ type: 'TAB_LOADED' });
                }
            }

            if (type === 'LOAD_STEMS') {
                if (this.engine) {
                    try {
                        const vocals = new Uint8Array(data.vocalsBytes);
                        const drums  = new Uint8Array(data.drumsBytes);
                        const bass   = new Uint8Array(data.bassBytes);
                        const other  = new Uint8Array(data.otherBytes);
                        this.engine.load_stems(vocals, drums, bass, other, data.tabJson);
                        this.stemMode = true;
                        this.port.postMessage({ type: 'STEMS_LOADED' });
                    } catch (e) {
                        this.port.postMessage({ type: 'STEMS_ERROR', error: e.toString() });
                    }
                }
            }

            if (type === 'SEEK') {
                if (this.engine) {
                    if (this.stemMode) {
                        this.engine.seek_stems_to_ms(data.position_ms);
                    } else {
                        this.engine.seek_to_ms(data.position_ms);
                    }
                }
            }

            if (type === 'UPDATE_PARAM') {
                if (this.engine) {
                    this.engine.set_node_parameter(
                        data.nodeId,
                        data.param,
                        data.value
                    );
                }
            }

            if (type === 'UPDATE_STEM_PARAM') {
                if (this.engine) {
                    this.engine.set_stem_node_parameter(
                        data.stemId,
                        data.nodeId,
                        data.param,
                        data.value
                    );
                }
            }

            if (type === 'SET_GLIDE_MS') {
                if (this.engine) {
                    this.engine.set_global_glide_ms(data.glide_ms);
                }
            }

            if (type === 'RESET') {
                if (this.engine) this.engine.reset();
            }

            if (type === 'UPDATE_TOPOLOGY') {
                if (data.topologyJson && this.engine) {
                    this.engine = new OpenClawEngine(
                        data.topologyJson,
                        data.blockSize,
                        sampleRate
                    );
                }
            }
        };
    }

    process(inputs, outputs) {
        if (!this.ready || !this.engine) {
            return true;
        }

        const input  = inputs[0];
        const output = outputs[0];

        if (!input[0] || !input[1]) return true;

        const blockSize = input[0].length;
        const interleaved = new Float32Array(blockSize * 2);
        for (let i = 0; i < blockSize; i++) {
            interleaved[i * 2]     = input[0][i];
            interleaved[i * 2 + 1] = input[1][i];
        }

        if (this.stemMode) {
            this.engine.process_stems(interleaved);
        } else if (this.tabLoaded) {
            this.engine.process_with_sections(interleaved);
        } else {
            this.engine.process(interleaved);
        }

        for (let i = 0; i < blockSize; i++) {
            output[0][i] = interleaved[i * 2];
            output[1][i] = interleaved[i * 2 + 1];
        }

        return true;
    }
}

registerProcessor('openclaw-worklet', OpenClawWorklet);
