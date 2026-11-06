import { afterTick, onDestroyEffect } from "svelte-toolbelt";
export class AnimationsComplete {
    #opts;
    #currentFrame = null;
    #observer = null;
    #runId = 0;
    constructor(opts) {
        this.#opts = opts;
        onDestroyEffect(() => this.#cleanup());
    }
    #cleanup() {
        if (this.#currentFrame !== null) {
            window.cancelAnimationFrame(this.#currentFrame);
            this.#currentFrame = null;
        }
        this.#observer?.disconnect();
        this.#observer = null;
        this.#runId++;
    }
    run(fn) {
        this.#cleanup();
        const runId = this.#runId;
        const executeIfCurrent = () => {
            if (runId !== this.#runId)
                return;
            this.#executeCallback(fn);
        };
        const waitForAnimations = () => {
            if (runId !== this.#runId)
                return;
            // The content element is only attached once the component has rendered,
            // which can happen after `run()` is called when opening.
            const node = this.#opts.ref.current;
            if (!node)
                return;
            if (typeof node.getAnimations !== "function") {
                this.#executeCallback(fn);
                return;
            }
            const animations = node.getAnimations();
            if (animations.length === 0) {
                executeIfCurrent();
                return;
            }
            Promise.all(animations.map((animation) => animation.finished))
                .then(() => {
                executeIfCurrent();
            })
                .catch(() => {
                if (runId !== this.#runId)
                    return;
                const currentAnimations = node.getAnimations();
                const hasRunningAnimations = currentAnimations.some((animation) => animation.pending || animation.playState !== "finished");
                if (hasRunningAnimations) {
                    waitForAnimations();
                    return;
                }
                executeIfCurrent();
            });
        };
        const requestWaitForAnimations = () => {
            this.#currentFrame = window.requestAnimationFrame(() => {
                this.#currentFrame = null;
                waitForAnimations();
            });
        };
        if (!this.#opts.afterTick.current) {
            requestWaitForAnimations();
            return;
        }
        this.#currentFrame = window.requestAnimationFrame(() => {
            this.#currentFrame = null;
            const startingStyleAttr = "data-starting-style";
            const node = this.#opts.ref.current;
            if (!node)
                return;
            if (!node.hasAttribute(startingStyleAttr)) {
                requestWaitForAnimations();
                return;
            }
            this.#observer = new MutationObserver(() => {
                if (runId !== this.#runId)
                    return;
                if (node.hasAttribute(startingStyleAttr))
                    return;
                this.#observer?.disconnect();
                this.#observer = null;
                requestWaitForAnimations();
            });
            this.#observer.observe(node, {
                attributes: true,
                attributeFilter: [startingStyleAttr],
            });
        });
    }
    #executeCallback(fn) {
        const runId = this.#runId;
        const execute = () => {
            // cleanup (destroy or a newer run) may have happened while `fn` was deferred
            if (runId !== this.#runId)
                return;
            fn();
        };
        if (this.#opts.afterTick.current) {
            afterTick(execute);
        }
        else {
            execute();
        }
    }
}
