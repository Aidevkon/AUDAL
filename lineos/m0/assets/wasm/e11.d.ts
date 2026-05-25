/* tslint:disable */
/* eslint-disable */

export class OpenClawEngine {
    free(): void;
    [Symbol.dispose](): void;
    current_position_ms(): number;
    load_stems(vocals_flac: Uint8Array, drums_flac: Uint8Array, bass_flac: Uint8Array, other_flac: Uint8Array, tab_json: string): void;
    load_time_aware_behaviour(tab_json: string): void;
    constructor(topology_json: string, block_size: number, sample_rate: number);
    process(input_output: Float32Array): void;
    process_stems(output: Float32Array): void;
    process_with_sections(input_output: Float32Array): void;
    reset(): void;
    seek_stems_to_ms(position_ms: number): void;
    seek_to_ms(position_ms: number): void;
    set_global_glide_ms(glide_ms: number): void;
    set_node_parameter(node_id: string, param: string, value: number): void;
    set_stem_node_parameter(stem_id: string, node_id: string, param: string, value: number): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_openclawengine_free: (a: number, b: number) => void;
    readonly openclawengine_current_position_ms: (a: number) => number;
    readonly openclawengine_load_stems: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => [number, number];
    readonly openclawengine_load_time_aware_behaviour: (a: number, b: number, c: number) => [number, number];
    readonly openclawengine_new: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly openclawengine_process: (a: number, b: number, c: number, d: any) => void;
    readonly openclawengine_process_stems: (a: number, b: number, c: number, d: any) => void;
    readonly openclawengine_process_with_sections: (a: number, b: number, c: number, d: any) => void;
    readonly openclawengine_reset: (a: number) => void;
    readonly openclawengine_seek_stems_to_ms: (a: number, b: number) => void;
    readonly openclawengine_seek_to_ms: (a: number, b: number) => void;
    readonly openclawengine_set_global_glide_ms: (a: number, b: number) => void;
    readonly openclawengine_set_node_parameter: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly openclawengine_set_stem_node_parameter: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
