/* tslint:disable */
/* eslint-disable */

/**
 * An opaque handle to a papercraft document (a model plus its current unfolding).
 */
export class PaperDoc {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Render the net to a multi-page vector PDF (cut/fold lines + glue flaps).
     */
    export_pdf(): Uint8Array;
    /**
     * Render the net to a single SVG laying every page out in the page grid.
     */
    export_svg(): Uint8Array;
    /**
     * Import a model from bytes and unfold it. `format` is the lowercase
     * extension without the dot (`stl`, `obj`, `pdo`, `glb`/`gltf`, `craft`).
     */
    constructor(bytes: Uint8Array, format: string);
    /**
     * Join the cut edge `edge` (an `EdgeIndex` from `model3d`/`pieces2d`).
     * Returns `true` if anything changed.
     */
    join_edge(edge: number): boolean;
    /**
     * The 3D mesh: `{ positions, normals, indices, edges }`.
     */
    model3d(): any;
    /**
     * Number of pieces (islands) in the current unfolding.
     */
    num_islands(): number;
    /**
     * Re-pack the islands onto the page(s). Returns the number of pages used.
     */
    pack_islands(): number;
    /**
     * The 2D net: `{ pieces: [{ id, triangles, cuts, folds }] }`.
     */
    pieces2d(): any;
    /**
     * Serialize the document to the `.craft` project format.
     */
    save_craft(): Uint8Array;
    /**
     * Split (cut) the joined edge `edge`. Returns `true` if anything changed.
     */
    split_edge(edge: number): boolean;
    /**
     * Produce an initial net by auto-joining edges across the whole model.
     * Use after importing raw geometry (STL/OBJ/glTF); a loaded `.craft` already
     * has its own unfolding. Returns the resulting number of pieces.
     */
    unwrap(): number;
}

/**
 * Install a panic hook that forwards Rust panics to `console.error`. Called
 * automatically when the module is instantiated.
 */
export function start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_paperdoc_free: (a: number, b: number) => void;
    readonly paperdoc_export_pdf: (a: number, b: number) => void;
    readonly paperdoc_export_svg: (a: number, b: number) => void;
    readonly paperdoc_import: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly paperdoc_join_edge: (a: number, b: number) => number;
    readonly paperdoc_model3d: (a: number, b: number) => void;
    readonly paperdoc_num_islands: (a: number) => number;
    readonly paperdoc_pack_islands: (a: number) => number;
    readonly paperdoc_pieces2d: (a: number, b: number) => void;
    readonly paperdoc_save_craft: (a: number, b: number) => void;
    readonly paperdoc_split_edge: (a: number, b: number) => number;
    readonly paperdoc_unwrap: (a: number) => number;
    readonly start: () => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
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
