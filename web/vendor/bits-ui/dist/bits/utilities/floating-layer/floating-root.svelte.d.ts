import { type ReadableBox, type ReadableBoxedValues } from "svelte-toolbelt";
import { Context } from "runed";
import type { Measurable } from "../../../internal/floating-svelte/types.js";
export declare const FloatingRootContext: Context<FloatingRootState>;
export declare const FloatingTooltipRootContext: Context<FloatingRootState>;
export declare class FloatingRootState {
    static create(tooltip?: boolean): FloatingRootState;
    anchorNode: import("svelte-toolbelt").WritableBox<HTMLElement | Measurable | null>;
    customAnchorNode: import("svelte-toolbelt").WritableBox<string | HTMLElement | Measurable | null>;
    triggerNode: ReadableBox<Measurable | HTMLElement | null>;
    constructor();
}
interface FloatingAnchorStateOpts extends ReadableBoxedValues<{
    id: string;
    virtualEl?: Measurable | null;
    ref: Measurable | HTMLElement | null;
}> {
}
export declare class FloatingAnchorState {
    static create(opts: FloatingAnchorStateOpts, tooltip?: boolean): FloatingAnchorState;
    readonly opts: FloatingAnchorStateOpts;
    readonly root: FloatingRootState;
    constructor(opts: FloatingAnchorStateOpts, root: FloatingRootState);
}
export {};
