import { afterTick, executeCallbacks, onDestroyEffect, } from "svelte-toolbelt";
import { watch } from "runed";
import { untrack } from "svelte";
import { on } from "svelte/events";
import {} from "../../../internal/events.js";
import { debounce } from "../../../internal/debounce.js";
import { noop } from "../../../internal/noop.js";
import { getOwnerDocument, isOrContainsTarget } from "../../../internal/elements.js";
import { isElementOrSVGElement } from "../../../internal/is.js";
import { isClickTrulyOutside } from "../../../internal/dom.js";
import { CONTEXT_MENU_CONTENT_ATTR, CONTEXT_MENU_TRIGGER_ATTR, } from "../../menu/context-menu-attributes.js";
globalThis.bitsDismissableLayers ??= new Map();
export class DismissibleLayerState {
    static create(opts) {
        return new DismissibleLayerState(opts);
    }
    opts;
    #interactOutsideProp;
    #behaviorType;
    #interceptedEvents = {
        pointerdown: false,
    };
    #isResponsibleLayer = false;
    #capturedPointerDown = null;
    #isFocusInsideDOMTree = false;
    #documentObj = undefined;
    #onFocusOutside;
    #unsubClickListener = noop;
    /**
     * Set once the layer is torn down. Deferred work scheduled before teardown can
     * still run afterwards, so every such callback must short-circuit on this
     * before reading `this.opts.ref.current` — reading a destroyed `$derived`
     * triggers Svelte's `derived_inert` warning. A class field rather than a
     * constructor local so the class-field handlers below can see it too.
     */
    #destroyed = false;
    constructor(opts) {
        this.opts = opts;
        this.#behaviorType = opts.interactOutsideBehavior;
        this.#interactOutsideProp = opts.onInteractOutside;
        this.#onFocusOutside = opts.onFocusOutside;
        let unsubEvents = noop;
        let registeredNode = null;
        const cleanup = () => {
            registeredNode = null;
            this.#resetState();
            globalThis.bitsDismissableLayers.delete(this);
            this.#handleInteractOutside.destroy();
            this.#unsubClickListener();
            unsubEvents();
        };
        watch([() => this.opts.enabled.current, () => this.opts.ref.current], ([enabled, node]) => {
            const nextNode = enabled ? node : null;
            // Ref boxes can invalidate without changing the DOM node during opening.
            // Keep an in-flight interaction when this is still the same registration.
            if (registeredNode === nextNode)
                return;
            cleanup();
            if (!nextNode)
                return;
            registeredNode = nextNode;
            this.#documentObj = getOwnerDocument(nextNode);
            globalThis.bitsDismissableLayers.set(this, this.#behaviorType);
            unsubEvents = this.#addEventListeners();
        });
        onDestroyEffect(() => {
            this.#destroyed = true;
            cleanup();
        });
    }
    #handleFocus = (event) => {
        if (event.defaultPrevented)
            return;
        if (this.#destroyed || !this.opts.ref.current)
            return;
        afterTick(() => {
            // The layer can be destroyed between the focus event and this tick — a
            // focus change is frequently what closes it. `#destroyed` must
            // short-circuit before any `this.opts.ref.current` read.
            if (this.#destroyed)
                return;
            if (!this.opts.ref.current || this.#isTargetWithinLayer(event.target))
                return;
            if (event.target && !this.#isFocusInsideDOMTree) {
                this.#onFocusOutside.current?.(event);
            }
        });
    };
    #addEventListeners() {
        const document = this.#documentObj;
        const onPointerDownCapture = (event) => {
            untrack(() => {
                // settle the previous interaction before this one overwrites its state
                this.#handleInteractOutside.flush();
                this.#markInterceptedEvent(event);
                this.#markResponsibleLayer(event);
            });
        };
        const onPointerDown = (event) => {
            // Ignore stopped events and bubble-only events such as the opening pointerdown.
            // It can resume bubbling after a nested early outside click has been captured.
            if (event.cancelBubble || event !== this.#capturedPointerDown)
                return;
            this.#markNonInterceptedEvent(event);
            if (event.pointerType === "touch") {
                // a tap's click can follow its pointerdown within a millisecond (WebKit), so
                // arm the click listener now rather than after the debounce flushes
                this.#handleInteractOutside.destroy();
                this.#interactOutside(event);
            }
            else {
                this.#handleInteractOutside(event);
            }
        };
        // `svelte/events.on` defers pointer listener attachment to a microtask, leaving
        // a gap even without a registration timer. Document is already connected, so
        // attach immediately. Only layers present at capture can own this interaction;
        // a layer opened by its target/bubble handlers must not dismiss itself.
        document.addEventListener("pointerdown", onPointerDownCapture, true);
        document.addEventListener("pointerdown", onPointerDown);
        return executeCallbacks(() => document.removeEventListener("pointerdown", onPointerDownCapture, true), () => document.removeEventListener("pointerdown", onPointerDown), on(document, "focusin", this.#handleFocus));
    }
    #handleDismiss = (e) => {
        let event = e;
        if (event.defaultPrevented) {
            event = createWrappedEvent(e);
        }
        this.#interactOutsideProp.current(e);
    };
    #interactOutside = (e) => {
        if (!this.opts.ref.current) {
            this.#unsubClickListener();
            return;
        }
        const isEventValid = this.opts.isValidEvent.current(e, this.opts.ref.current) ||
            isValidEvent(e, this.opts.ref.current);
        if (!this.#isResponsibleLayer || this.#isAnyEventIntercepted() || !isEventValid) {
            this.#unsubClickListener();
            return;
        }
        let event = e;
        if (event.defaultPrevented) {
            event = createWrappedEvent(event);
        }
        if (this.#behaviorType.current !== "close" &&
            this.#behaviorType.current !== "defer-otherwise-close") {
            this.#unsubClickListener();
            return;
        }
        if (e.pointerType === "touch") {
            this.#unsubClickListener();
            // the tap's click dismisses from the bubble phase, as before. A capture-phase
            // listener backs it up with a task, so a target that stops propagation cannot
            // hide the click, while a target that closes this layer still tears it down
            // (and the backup with it) before the task runs.
            let backup = null;
            let unsubClick = noop;
            const dismiss = (event) => {
                unsubClick();
                this.#handleDismiss(event);
            };
            unsubClick = executeCallbacks(on(this.#documentObj, "click", dismiss, { once: true }), on(this.#documentObj, "click", (event) => {
                backup = setTimeout(() => dismiss(event), 0);
            }, { once: true, capture: true }), () => {
                if (backup !== null)
                    clearTimeout(backup);
                backup = null;
            });
            this.#unsubClickListener = unsubClick;
        }
        else {
            this.#interactOutsideProp.current(event);
        }
    };
    #handleInteractOutside = debounce(this.#interactOutside, 10);
    #markInterceptedEvent = (e) => {
        this.#interceptedEvents[e.type] = true;
    };
    #markNonInterceptedEvent = (e) => {
        this.#interceptedEvents[e.type] = false;
    };
    #markResponsibleLayer = (event) => {
        this.#capturedPointerDown = event;
        if (!this.opts.ref.current)
            return;
        this.#isResponsibleLayer = isResponsibleLayer(this.opts.ref.current);
    };
    #isTargetWithinLayer = (target) => {
        if (!this.opts.ref.current)
            return false;
        return isOrContainsTarget(this.opts.ref.current, target);
    };
    /**
     * Resets the per-interaction state. Must stay synchronous.
     *
     * This was a `debounce(..., 20)` from when it was also wired to a capture-phase
     * interaction-end listener and had to land after the 10ms `#handleInteractOutside`
     * debounce. That listener is gone, but the debounce stayed on the `cleanup()` path —
     * and because `watch` runs `cleanup()` once per open (`ref` goes null -> node), every
     * layer scheduled a reset 20ms into its own lifetime. An outside `pointerdown` landing
     * 10-20ms after that cleanup would have its `#isResponsibleLayer` flag cleared by the
     * stale reset in the gap before the debounced `#handleInteractOutside` ran, which then
     * bailed and left the layer open. `cleanup()` destroys `#handleInteractOutside` anyway,
     * so nothing is left in flight that needs to observe the pre-reset state.
     */
    #resetState = () => {
        for (const eventType in this.#interceptedEvents) {
            this.#interceptedEvents[eventType] = false;
        }
        this.#isResponsibleLayer = false;
        this.#capturedPointerDown = null;
    };
    #isAnyEventIntercepted() {
        const i = Object.values(this.#interceptedEvents).some(Boolean);
        return i;
    }
    #onfocuscapture = () => {
        this.#isFocusInsideDOMTree = true;
    };
    #onblurcapture = () => {
        this.#isFocusInsideDOMTree = false;
    };
    props = {
        onfocuscapture: this.#onfocuscapture,
        onblurcapture: this.#onblurcapture,
    };
}
export function getTopMostDismissableLayer(layersArr = [
    ...globalThis.bitsDismissableLayers,
]) {
    return layersArr.findLast(([_, { current: behaviorType }]) => behaviorType === "close" || behaviorType === "ignore");
}
function isResponsibleLayer(node) {
    const layersArr = [...globalThis.bitsDismissableLayers];
    /**
     * We first check if we can find a top layer with `close` or `ignore`.
     * If that top layer was found and matches the provided node, then the node is
     * responsible for the outside interaction. Otherwise, we know that all layers defer so
     * the first layer is the responsible one.
     */
    const topMostLayer = getTopMostDismissableLayer(layersArr);
    if (topMostLayer)
        return topMostLayer[0].opts.ref.current === node;
    const [firstLayerNode] = layersArr[0];
    return firstLayerNode.opts.ref.current === node;
}
function isValidEvent(e, node) {
    const target = e.target;
    if (!isElementOrSVGElement(target))
        return false;
    const targetIsContextMenuTrigger = Boolean(target.closest(`[${CONTEXT_MENU_TRIGGER_ATTR}]`));
    const nodeIsContextMenu = Boolean(node.closest(`[${CONTEXT_MENU_CONTENT_ATTR}]`));
    if ("button" in e && e.button > 0 && !targetIsContextMenuTrigger)
        return false;
    if ("button" in e && e.button === 0 && targetIsContextMenuTrigger && nodeIsContextMenu) {
        return true;
    }
    if (targetIsContextMenuTrigger && nodeIsContextMenu)
        return false;
    const ownerDocument = getOwnerDocument(target);
    const isValid = ownerDocument.documentElement.contains(target) &&
        !isOrContainsTarget(node, target) &&
        isClickTrulyOutside(e, node);
    return isValid;
}
function createWrappedEvent(e) {
    const capturedCurrentTarget = e.currentTarget;
    const capturedTarget = e.target;
    let newEvent;
    if (e instanceof PointerEvent) {
        newEvent = new PointerEvent(e.type, e);
    }
    else {
        newEvent = new PointerEvent("pointerdown", e);
    }
    // track the prevented state separately
    let isPrevented = false;
    // Create a proxy to intercept property access and method calls
    const wrappedEvent = new Proxy(newEvent, {
        get: (target, prop) => {
            if (prop === "currentTarget") {
                return capturedCurrentTarget;
            }
            if (prop === "target") {
                return capturedTarget;
            }
            if (prop === "preventDefault") {
                return () => {
                    isPrevented = true;
                    if (typeof target.preventDefault === "function") {
                        target.preventDefault();
                    }
                };
            }
            if (prop === "defaultPrevented") {
                return isPrevented;
            }
            if (prop in target) {
                // oxlint-disable-next-line no-explicit-any
                return target[prop];
            }
            // oxlint-disable-next-line no-explicit-any
            return e[prop];
        },
    });
    return wrappedEvent;
}
