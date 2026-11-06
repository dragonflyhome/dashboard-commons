/**
 * Keeps content that mounts open from playing its open animation/transition.
 *
 * The block is rendered through the content's `style` prop rather than set on the
 * node imperatively, since Svelte rewrites the `style` attribute whenever the
 * content's props change (e.g. after measuring its dimensions). It lifts on the
 * first open state change, so later opening and closing still animate.
 */
export declare class MountAnimationGuard {
    #private;
    constructor(open: () => boolean);
    get style(): {
        readonly animationName: "none";
        readonly transitionDuration: "0s";
    } | undefined;
}
/**
 * Measures `node` at its full size with animations and transitions blocked, then
 * restores its inline styles.
 */
export declare function measureWithoutAnimation(node: HTMLElement): DOMRect;
