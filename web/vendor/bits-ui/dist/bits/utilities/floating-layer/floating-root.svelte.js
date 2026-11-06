import { boxFrom, simpleBox } from "svelte-toolbelt";
import { Context } from "runed";
export const FloatingRootContext = new Context("Floating.Root");
export const FloatingTooltipRootContext = new Context("Floating.Root");
export class FloatingRootState {
    static create(tooltip = false) {
        return tooltip
            ? FloatingTooltipRootContext.set(new FloatingRootState())
            : FloatingRootContext.set(new FloatingRootState());
    }
    anchorNode = simpleBox(null);
    customAnchorNode = simpleBox(null);
    triggerNode = simpleBox(null);
    constructor() {
        $effect(() => {
            if (this.customAnchorNode.current) {
                if (typeof this.customAnchorNode.current === "string") {
                    this.anchorNode.current = document.querySelector(this.customAnchorNode.current);
                }
                else {
                    this.anchorNode.current = this.customAnchorNode.current;
                }
            }
            else {
                this.anchorNode.current = this.triggerNode.current;
            }
        });
    }
}
export class FloatingAnchorState {
    static create(opts, tooltip = false) {
        return tooltip
            ? new FloatingAnchorState(opts, FloatingTooltipRootContext.get())
            : new FloatingAnchorState(opts, FloatingRootContext.get());
    }
    opts;
    root;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        if (opts.virtualEl && opts.virtualEl.current) {
            root.triggerNode = boxFrom(opts.virtualEl.current);
        }
        else {
            root.triggerNode = opts.ref;
        }
    }
}
