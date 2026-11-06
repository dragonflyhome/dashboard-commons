import { SvelteMap } from "svelte/reactivity";
import { Context, Previous, watch } from "runed";
import { afterTick, onDestroyEffect, attachRef, DOMContext, executeCallbacks, boxWith, } from "svelte-toolbelt";
import { on } from "svelte/events";
import { backward, forward, next, prev } from "../../internal/arrays.js";
import { boolToStr, boolToStrTrueOrUndef, boolToEmptyStrOrUndef, getDataOpenClosed, boolToTrueOrUndef, getDataTransitionAttrs, } from "../../internal/attrs.js";
import { kbd } from "../../internal/kbd.js";
import { noop } from "../../internal/noop.js";
import { isDisabledElement, isIOS } from "../../internal/is.js";
import { createBitsAttrs } from "../../internal/attrs.js";
import { getFloatingContentCSSVars } from "../../internal/floating-svelte/floating-utils.svelte.js";
import { DataTypeahead } from "../../internal/data-typeahead.svelte.js";
import { DOMTypeahead } from "../../internal/dom-typeahead.svelte.js";
import { PresenceManager } from "../../internal/presence-manager.svelte.js";
import { DEV } from "esm-env";
// prettier-ignore
export const INTERACTION_KEYS = [kbd.ARROW_LEFT, kbd.ESCAPE, kbd.ARROW_RIGHT, kbd.SHIFT, kbd.CAPS_LOCK, kbd.CONTROL, kbd.ALT, kbd.META, kbd.ENTER, kbd.F1, kbd.F2, kbd.F3, kbd.F4, kbd.F5, kbd.F6, kbd.F7, kbd.F8, kbd.F9, kbd.F10, kbd.F11, kbd.F12];
export const FIRST_KEYS = [kbd.ARROW_DOWN, kbd.PAGE_UP, kbd.HOME];
export const LAST_KEYS = [kbd.ARROW_UP, kbd.PAGE_DOWN, kbd.END];
export const FIRST_LAST_KEYS = [...FIRST_KEYS, ...LAST_KEYS];
export const SELECTION_KEYS = [kbd.ENTER, kbd.SPACE];
export const CONTENT_MARGIN = 10;
const selectAttrs = createBitsAttrs({
    component: "select",
    parts: [
        "trigger",
        "content",
        "item",
        "viewport",
        "scroll-up-button",
        "scroll-down-button",
        "group",
        "group-label",
        "separator",
        "arrow",
        "input",
        "content-wrapper",
        "item-text",
        "value",
    ],
});
const SelectRootContext = new Context("Select.Root | Combobox.Root");
const SelectGroupContext = new Context("Select.Group | Combobox.Group");
const SelectContentContext = new Context("Select.Content | Combobox.Content");
class SelectBaseRootState {
    opts;
    touchedInput = $state(false);
    inputNode = $state(null);
    contentNode = $state(null);
    contentPresence;
    viewportNode = $state(null);
    triggerNode = $state(null);
    valueNode = $state(null);
    valueId = $state("");
    /**
     * Labels for selected values, populated when items are toggled or resolved from
     * mounted item nodes. Used so `<Select.Value>` keeps labels after the menu closes.
     */
    selectedLabelsByValue = new SvelteMap();
    highlightedNode = $state(null);
    highlightedValue = $derived.by(() => {
        if (!this.highlightedNode)
            return null;
        return this.highlightedNode.getAttribute("data-value");
    });
    highlightedId = $derived.by(() => {
        if (!this.highlightedNode)
            return undefined;
        return this.highlightedNode.id;
    });
    highlightedLabel = $derived.by(() => {
        if (!this.highlightedNode)
            return null;
        return this.highlightedNode.getAttribute("data-label");
    });
    contentIsPositioned = $state(false);
    isUsingKeyboard = false;
    /**
     * The item a press started on, and where. The list can scroll under a still
     * pointer before the release, which then lands on a neighbouring item.
     */
    pressedItem = null;
    isCombobox = false;
    domContext = new DOMContext(() => null);
    constructor(opts) {
        this.opts = opts;
        this.isCombobox = opts.isCombobox;
        this.contentPresence = new PresenceManager({
            ref: boxWith(() => this.contentNode),
            open: this.opts.open,
            onComplete: () => {
                this.opts.onOpenChangeComplete.current(this.opts.open.current);
            },
        });
        $effect.pre(() => {
            if (!this.opts.open.current) {
                this.setHighlightedNode(null);
                this.pressedItem = null;
            }
        });
    }
    setHighlightedNode(node, initial = false) {
        this.highlightedNode = node;
        if (node && (this.isUsingKeyboard || initial)) {
            this.scrollHighlightedNodeIntoView(node);
        }
    }
    scrollHighlightedNodeIntoView(node) {
        if (!this.viewportNode || !this.contentIsPositioned)
            return;
        node.scrollIntoView({ block: this.opts.scrollAlignment.current });
    }
    getCandidateNodes() {
        const node = this.contentNode;
        if (!node)
            return [];
        return Array.from(node.querySelectorAll(`[${this.getBitsAttr("item")}]:not([data-disabled])`));
    }
    setHighlightedToFirstCandidate(initial = false) {
        this.setHighlightedNode(null);
        let nodes = this.getCandidateNodes();
        if (!nodes.length)
            return;
        // don't consider nodes that aren't visible within the viewport
        if (this.viewportNode) {
            const viewportRect = this.viewportNode.getBoundingClientRect();
            nodes = nodes.filter((node) => {
                if (!this.viewportNode)
                    return false;
                const nodeRect = node.getBoundingClientRect();
                const isNodeFullyVisible = nodeRect.right <= viewportRect.right &&
                    nodeRect.left >= viewportRect.left &&
                    nodeRect.bottom <= viewportRect.bottom &&
                    nodeRect.top >= viewportRect.top;
                return isNodeFullyVisible;
            });
        }
        this.setHighlightedNode(nodes[0], initial);
    }
    getNodeByValue(value) {
        const candidateNodes = this.getCandidateNodes();
        return candidateNodes.find((node) => node.dataset.value === value) ?? null;
    }
    /**
     * Resolves the display label for a value: a non-empty `items` entry, otherwise the
     * label cached from the selected item. Never reads the DOM, so it stays reactive and
     * works the same whether items are mounted, unmounted, or force-mounted.
     */
    getLabelForValue(value) {
        if (value === "")
            return "";
        const fromItems = this.opts.items.current.find((item) => item.value === value)?.label;
        if (fromItems?.trim())
            return fromItems;
        return this.selectedLabelsByValue.get(value) ?? value;
    }
    /** The item's `label`, falling back to its text content when the label is empty. */
    resolveItemLabel(value, label, node) {
        if (label?.trim())
            return label;
        return node?.textContent?.trim() || value;
    }
    setSelectedLabel(value, label) {
        if (value === "" || label === value)
            this.selectedLabelsByValue.delete(value);
        else
            this.selectedLabelsByValue.set(value, label);
    }
    pruneSelectedLabels(keptValues) {
        for (const value of this.selectedLabelsByValue.keys()) {
            if (!keptValues.includes(value))
                this.selectedLabelsByValue.delete(value);
        }
    }
    setOpen(open) {
        this.opts.open.current = open;
    }
    toggleOpen() {
        this.opts.open.current = !this.opts.open.current;
    }
    handleOpen() {
        this.setOpen(true);
    }
    handleClose() {
        this.setHighlightedNode(null);
        this.setOpen(false);
    }
    toggleMenu() {
        this.toggleOpen();
    }
    getBitsAttr = (part) => {
        return selectAttrs.getAttr(part, this.isCombobox ? "combobox" : undefined);
    };
}
export class SelectSingleRootState extends SelectBaseRootState {
    opts;
    isMulti = false;
    hasValue = $derived.by(() => this.opts.value.current !== "");
    currentLabel = $derived.by(() => {
        if (!this.opts.items.current.length)
            return "";
        return (this.opts.items.current.find((item) => item.value === this.opts.value.current)?.label ??
            "");
    });
    candidateLabels = $derived.by(() => {
        if (!this.opts.items.current.length)
            return [];
        const filteredItems = this.opts.items.current.filter((item) => !item.disabled);
        return filteredItems.map((item) => item.label);
    });
    dataTypeaheadEnabled = $derived.by(() => {
        if (this.isMulti)
            return false;
        if (this.opts.items.current.length === 0)
            return false;
        return true;
    });
    constructor(opts) {
        super(opts);
        this.opts = opts;
        $effect(() => {
            if (!this.opts.open.current && this.highlightedNode) {
                this.setHighlightedNode(null);
            }
        });
        watch(() => this.opts.open.current, () => {
            if (!this.opts.open.current)
                return;
            this.setInitialHighlightedNode();
        });
        watch(() => this.opts.value.current, (value) => {
            this.pruneSelectedLabels(value === "" ? [] : [value]);
        });
    }
    includesItem(itemValue) {
        return this.opts.value.current === itemValue;
    }
    toggleItem(itemValue, itemLabel = itemValue) {
        const newValue = this.includesItem(itemValue) ? "" : itemValue;
        this.opts.value.current = newValue;
        if (newValue !== "") {
            this.setSelectedLabel(itemValue, itemLabel);
            this.opts.inputValue.current = itemLabel;
        }
    }
    setInitialHighlightedNode() {
        afterTick(() => {
            if (this.highlightedNode &&
                this.domContext.getDocument().contains(this.highlightedNode))
                return;
            // "" is a valid value for a select item so we need to look it up too
            const node = this.getNodeByValue(this.opts.value.current);
            if (node) {
                this.setHighlightedNode(node, true);
                return;
            }
            // if no item matches the value, we want to highlight the first item
            this.setHighlightedToFirstCandidate(true);
        });
    }
}
class SelectMultipleRootState extends SelectBaseRootState {
    opts;
    isMulti = true;
    hasValue = $derived.by(() => this.opts.value.current.length > 0);
    /**
     * `includesItem` is called once per mounted item whenever the value changes, so we keep a
     * set around to avoid a linear scan of `value` per item.
     */
    #valueSet = $derived.by(() => new Set(this.opts.value.current));
    constructor(opts) {
        super(opts);
        this.opts = opts;
        $effect(() => {
            if (!this.opts.open.current && this.highlightedNode) {
                this.setHighlightedNode(null);
            }
        });
        watch(() => this.opts.open.current, () => {
            if (!this.opts.open.current)
                return;
            this.setInitialHighlightedNode();
        });
        watch(() => this.opts.value.current, (value) => {
            this.pruneSelectedLabels(value);
        });
    }
    includesItem(itemValue) {
        return this.#valueSet.has(itemValue);
    }
    toggleItem(itemValue, itemLabel = itemValue) {
        if (this.includesItem(itemValue)) {
            this.opts.value.current = this.opts.value.current.filter((v) => v !== itemValue);
        }
        else {
            this.opts.value.current = [...this.opts.value.current, itemValue];
            this.setSelectedLabel(itemValue, itemLabel);
        }
        this.opts.inputValue.current = itemLabel;
    }
    setInitialHighlightedNode() {
        afterTick(() => {
            if (!this.domContext)
                return;
            if (this.highlightedNode &&
                this.domContext.getDocument().contains(this.highlightedNode))
                return;
            if (this.opts.value.current.length) {
                const node = this.getNodeByValue(this.opts.value.current[0]);
                if (node) {
                    this.setHighlightedNode(node, true);
                    return;
                }
            }
            // if no value is set, we want to highlight the first item
            this.setHighlightedToFirstCandidate(true);
        });
    }
}
export class SelectRootState {
    static create(props) {
        const { type, ...rest } = props;
        const rootState = type === "single"
            ? new SelectSingleRootState(rest)
            : new SelectMultipleRootState(rest);
        return SelectRootContext.set(rootState);
    }
}
export class SelectValueState {
    static create(opts) {
        return new SelectValueState(opts, SelectRootContext.get());
    }
    root;
    opts;
    attachment;
    constructor(opts, root) {
        this.root = root;
        this.opts = opts;
        this.attachment = attachRef(opts.ref, (v) => (this.root.valueNode = v));
        this.setValue = this.setValue.bind(this);
    }
    setValue(value) {
        if (this.root.isMulti && !Array.isArray(value)) {
            if (DEV)
                throw new Error(`Expected an array of strings passed to \`setValue\` got ${typeof value}.`);
            return;
        }
        if (!this.root.isMulti && typeof value !== "string") {
            if (DEV)
                throw new Error(`Expected a string passed to \`setValue\` got ${typeof value}.`);
            return;
        }
        // stale cached labels are pruned by the root's value watcher
        this.root.opts.value.current = value;
    }
    // this way consumers get type narrowing for the value on `type`
    snippetProps = $derived.by(() => {
        if (this.root.isMulti) {
            return {
                selection: {
                    type: "multiple",
                    selected: this.root.opts.value.current.length > 0
                        ? this.root.opts.value.current.map((value) => ({
                            value,
                            label: this.root.getLabelForValue(value),
                        }))
                        : [],
                    setValue: this.setValue,
                },
                placeholder: this.opts.placeholder.current ?? null,
                disabled: this.root.opts.disabled.current,
            };
        }
        const value = this.root.opts.value.current;
        return {
            selection: {
                type: "single",
                selected: value !== "" ? { value, label: this.root.getLabelForValue(value) } : undefined,
                setValue: this.setValue,
            },
            placeholder: this.opts.placeholder.current ?? null,
            disabled: this.root.opts.disabled.current,
        };
    });
    props = $derived.by(() => ({
        id: this.opts.id.current,
        "data-placeholder": this.root.hasValue ? undefined : "",
        "data-select-value": "",
        ...this.attachment,
    }));
}
export class SelectInputState {
    static create(opts) {
        return new SelectInputState(opts, SelectRootContext.get());
    }
    opts;
    root;
    attachment;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.attachment = attachRef(opts.ref, (v) => (this.root.inputNode = v));
        this.root.domContext = new DOMContext(opts.ref);
        this.onkeydown = this.onkeydown.bind(this);
        this.oninput = this.oninput.bind(this);
        watch([() => this.root.opts.value.current, () => this.opts.clearOnDeselect.current], ([value, clearOnDeselect], [prevValue]) => {
            if (!clearOnDeselect)
                return;
            if (Array.isArray(value) && Array.isArray(prevValue)) {
                if (value.length === 0 && prevValue.length !== 0) {
                    this.root.opts.inputValue.current = "";
                }
            }
            else if (value === "" && prevValue !== "") {
                this.root.opts.inputValue.current = "";
            }
        });
    }
    onkeydown(e) {
        this.root.isUsingKeyboard = true;
        if (e.key === kbd.ESCAPE)
            return;
        // Home/End move the text cursor in the input, per the WAI-ARIA combobox pattern
        if (e.key === kbd.HOME || e.key === kbd.END)
            return;
        // prevent arrow up/down from moving the position of the cursor in the input
        if (e.key === kbd.ARROW_UP || e.key === kbd.ARROW_DOWN)
            e.preventDefault();
        if (!this.root.opts.open.current) {
            if (INTERACTION_KEYS.includes(e.key))
                return;
            if (e.key === kbd.TAB)
                return;
            if (e.key === kbd.BACKSPACE && this.root.opts.inputValue.current === "")
                return;
            this.root.handleOpen();
            // we need to wait for a tick after the menu opens to ensure the highlighted nodes are
            // set correctly.
            if (this.root.hasValue)
                return;
            const candidateNodes = this.root.getCandidateNodes();
            if (!candidateNodes.length)
                return;
            if (e.key === kbd.ARROW_DOWN) {
                const firstCandidate = candidateNodes[0];
                this.root.setHighlightedNode(firstCandidate);
            }
            else if (e.key === kbd.ARROW_UP) {
                const lastCandidate = candidateNodes[candidateNodes.length - 1];
                this.root.setHighlightedNode(lastCandidate);
            }
            return;
        }
        if (e.key === kbd.TAB) {
            this.root.handleClose();
            return;
        }
        if (e.key === kbd.ENTER && !e.isComposing) {
            e.preventDefault();
            const isCurrentSelectedValue = this.root.highlightedValue === this.root.opts.value.current;
            if (!this.root.opts.allowDeselect.current &&
                isCurrentSelectedValue &&
                !this.root.isMulti) {
                this.root.handleClose();
                return;
            }
            if (this.root.highlightedValue !== null &&
                this.root.highlightedNode &&
                this.root.highlightedNode.isConnected) {
                this.root.toggleItem(this.root.highlightedValue, this.root.resolveItemLabel(this.root.highlightedValue, this.root.highlightedLabel, this.root.highlightedNode));
            }
            if (!this.root.isMulti && !isCurrentSelectedValue) {
                this.root.handleClose();
            }
        }
        if (e.key === kbd.ARROW_UP && e.altKey) {
            this.root.handleClose();
        }
        if (FIRST_LAST_KEYS.includes(e.key)) {
            e.preventDefault();
            const candidateNodes = this.root.getCandidateNodes();
            const currHighlightedNode = this.root.highlightedNode;
            const currIndex = currHighlightedNode
                ? candidateNodes.indexOf(currHighlightedNode)
                : -1;
            const loop = this.root.opts.loop.current;
            let nextItem;
            if (e.key === kbd.ARROW_DOWN) {
                nextItem = next(candidateNodes, currIndex, loop);
            }
            else if (e.key === kbd.ARROW_UP) {
                nextItem = prev(candidateNodes, currIndex, loop);
            }
            else if (e.key === kbd.PAGE_DOWN) {
                nextItem = forward(candidateNodes, currIndex, 10, loop);
            }
            else if (e.key === kbd.PAGE_UP) {
                nextItem = backward(candidateNodes, currIndex, 10, loop);
            }
            else if (e.key === kbd.HOME) {
                nextItem = candidateNodes[0];
            }
            else if (e.key === kbd.END) {
                nextItem = candidateNodes[candidateNodes.length - 1];
            }
            if (!nextItem)
                return;
            this.root.setHighlightedNode(nextItem);
            return;
        }
        if (INTERACTION_KEYS.includes(e.key))
            return;
        if (!this.root.highlightedNode) {
            this.root.setHighlightedToFirstCandidate();
        }
    }
    oninput(e) {
        this.root.opts.inputValue.current = e.currentTarget.value;
        afterTick(() => {
            if (!this.root.opts.open.current)
                return;
            this.root.setHighlightedToFirstCandidate();
        });
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        role: "combobox",
        disabled: this.root.opts.disabled.current ? true : undefined,
        "aria-activedescendant": this.root.highlightedId,
        "aria-autocomplete": "list",
        "aria-expanded": boolToStr(this.root.opts.open.current),
        "data-state": getDataOpenClosed(this.root.opts.open.current),
        "data-disabled": boolToEmptyStrOrUndef(this.root.opts.disabled.current),
        onkeydown: this.onkeydown,
        oninput: this.oninput,
        [this.root.getBitsAttr("input")]: "",
        ...this.attachment,
    }));
}
export class SelectComboTriggerState {
    static create(opts) {
        return new SelectComboTriggerState(opts, SelectRootContext.get());
    }
    opts;
    root;
    attachment;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.attachment = attachRef(opts.ref);
        this.onkeydown = this.onkeydown.bind(this);
        this.onpointerdown = this.onpointerdown.bind(this);
        this.onpointerup = this.onpointerup.bind(this);
        this.onclick = this.onclick.bind(this);
    }
    #focusInputAndToggle() {
        if (!this.root.domContext)
            return;
        if (this.root.domContext.getActiveElement() !== this.root.inputNode) {
            this.root.inputNode?.focus();
        }
        this.root.toggleMenu();
    }
    onkeydown(e) {
        if (!this.root.domContext)
            return;
        if (e.key === kbd.ENTER || e.key === kbd.SPACE) {
            e.preventDefault();
            this.#focusInputAndToggle();
        }
    }
    /**
     * `pointerdown` fires before the `focus` event, so we can prevent the default
     * behavior of focusing the button and keep focus on the input.
     */
    onpointerdown(e) {
        if (this.root.opts.disabled.current ||
            isDisabledElement(this.opts.ref.current) ||
            !this.root.domContext)
            return;
        e.preventDefault();
        // prevent opening on touch down which can be triggered when scrolling on touch devices
        if (e.pointerType === "touch")
            return;
        this.#focusInputAndToggle();
    }
    onpointerup(e) {
        if (this.root.opts.disabled.current ||
            isDisabledElement(this.opts.ref.current) ||
            !this.root.domContext)
            return;
        if (e.pointerType !== "touch")
            return;
        e.preventDefault();
        this.#focusInputAndToggle();
    }
    /**
     * Screen readers can activate the trigger with a `click` that no pointer events
     * precede (VoiceOver in Safari). A click that ends a press has a non-zero `detail`
     * and was already handled by `onpointerdown` / `onpointerup`.
     */
    onclick(e) {
        if (this.root.opts.disabled.current || e.detail !== 0)
            return;
        this.#focusInputAndToggle();
        e.preventDefault();
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        disabled: this.root.opts.disabled.current ? true : undefined,
        "aria-haspopup": "listbox",
        "data-state": getDataOpenClosed(this.root.opts.open.current),
        "data-disabled": boolToEmptyStrOrUndef(this.root.opts.disabled.current),
        [this.root.getBitsAttr("trigger")]: "",
        onpointerdown: this.onpointerdown,
        onpointerup: this.onpointerup,
        onclick: this.onclick,
        onkeydown: this.onkeydown,
        ...this.attachment,
    }));
}
export class SelectTriggerState {
    static create(opts) {
        return new SelectTriggerState(opts, SelectRootContext.get());
    }
    opts;
    root;
    attachment;
    #domTypeahead;
    #dataTypeahead;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.attachment = attachRef(opts.ref, (v) => (this.root.triggerNode = v));
        this.root.domContext = new DOMContext(opts.ref);
        this.#domTypeahead = new DOMTypeahead({
            getCurrentItem: () => this.root.highlightedNode,
            onMatch: (node) => {
                this.root.setHighlightedNode(node);
            },
            getActiveElement: () => this.root.domContext.getActiveElement(),
            getWindow: () => this.root.domContext.getWindow(),
        });
        this.#dataTypeahead = new DataTypeahead({
            getCurrentItem: () => {
                if (this.root.isMulti)
                    return "";
                return this.root.currentLabel;
            },
            onMatch: (label) => {
                if (this.root.isMulti)
                    return;
                if (!this.root.opts.items.current)
                    return;
                const matchedItem = this.root.opts.items.current.find((item) => item.label === label);
                if (!matchedItem)
                    return;
                this.root.opts.value.current = matchedItem.value;
            },
            enabled: () => !this.root.isMulti && this.root.dataTypeaheadEnabled,
            candidateValues: () => (this.root.isMulti ? [] : this.root.candidateLabels),
            getWindow: () => this.root.domContext.getWindow(),
        });
        this.onkeydown = this.onkeydown.bind(this);
        this.onpointerdown = this.onpointerdown.bind(this);
        this.onpointerup = this.onpointerup.bind(this);
        this.onclick = this.onclick.bind(this);
    }
    #handleOpen() {
        this.root.opts.open.current = true;
        this.#dataTypeahead.resetTypeahead();
        this.#domTypeahead.resetTypeahead();
    }
    #handlePointerOpen(_) {
        this.#handleOpen();
    }
    /**
     * Logic used to handle keyboard selection/deselection.
     *
     * If it returns true, it means the item was selected and whatever is calling
     * this function should return early
     *
     */
    #handleKeyboardSelection() {
        const isCurrentSelectedValue = this.root.highlightedValue === this.root.opts.value.current;
        if (!this.root.opts.allowDeselect.current && isCurrentSelectedValue && !this.root.isMulti) {
            this.root.handleClose();
            return true;
        }
        // "" is a valid value for a select item so we need to check for that
        if (this.root.highlightedValue !== null) {
            this.root.toggleItem(this.root.highlightedValue, this.root.resolveItemLabel(this.root.highlightedValue, this.root.highlightedLabel, this.root.highlightedNode));
        }
        if (!this.root.isMulti && !isCurrentSelectedValue) {
            this.root.handleClose();
            return true;
        }
        return false;
    }
    onkeydown(e) {
        this.root.isUsingKeyboard = true;
        if (e.key === kbd.ARROW_UP || e.key === kbd.ARROW_DOWN)
            e.preventDefault();
        if (!this.root.opts.open.current) {
            if (e.key === kbd.ENTER ||
                e.key === kbd.SPACE ||
                e.key === kbd.ARROW_DOWN ||
                e.key === kbd.ARROW_UP) {
                e.preventDefault();
                this.root.handleOpen();
            }
            else if (!this.root.isMulti && this.root.dataTypeaheadEnabled) {
                this.#dataTypeahead.handleTypeaheadSearch(e.key);
                return;
            }
            // we need to wait for a tick after the menu opens to ensure
            // the highlighted nodes are set correctly
            if (this.root.hasValue)
                return;
            const candidateNodes = this.root.getCandidateNodes();
            if (!candidateNodes.length)
                return;
            if (e.key === kbd.ARROW_DOWN) {
                const firstCandidate = candidateNodes[0];
                this.root.setHighlightedNode(firstCandidate);
            }
            else if (e.key === kbd.ARROW_UP) {
                const lastCandidate = candidateNodes[candidateNodes.length - 1];
                this.root.setHighlightedNode(lastCandidate);
            }
            return;
        }
        if (e.key === kbd.TAB) {
            this.root.handleClose();
            return;
        }
        if ((e.key === kbd.ENTER ||
            // if we're currently "typing ahead", we don't want to select the item
            // just yet as the item the user is trying to get to may have a space in it,
            // so we defer handling the close for this case until further down
            (e.key === kbd.SPACE && this.#domTypeahead.search === "")) &&
            !e.isComposing) {
            e.preventDefault();
            const shouldReturn = this.#handleKeyboardSelection();
            if (shouldReturn || e.key === kbd.SPACE)
                return;
        }
        if (e.key === kbd.ARROW_UP && e.altKey) {
            this.root.handleClose();
        }
        if (FIRST_LAST_KEYS.includes(e.key)) {
            e.preventDefault();
            const candidateNodes = this.root.getCandidateNodes();
            const currHighlightedNode = this.root.highlightedNode;
            const currIndex = currHighlightedNode
                ? candidateNodes.indexOf(currHighlightedNode)
                : -1;
            const loop = this.root.opts.loop.current;
            let nextItem;
            if (e.key === kbd.ARROW_DOWN) {
                nextItem = next(candidateNodes, currIndex, loop);
            }
            else if (e.key === kbd.ARROW_UP) {
                nextItem = prev(candidateNodes, currIndex, loop);
            }
            else if (e.key === kbd.PAGE_DOWN) {
                nextItem = forward(candidateNodes, currIndex, 10, loop);
            }
            else if (e.key === kbd.PAGE_UP) {
                nextItem = backward(candidateNodes, currIndex, 10, loop);
            }
            else if (e.key === kbd.HOME) {
                nextItem = candidateNodes[0];
            }
            else if (e.key === kbd.END) {
                nextItem = candidateNodes[candidateNodes.length - 1];
            }
            if (!nextItem)
                return;
            this.root.setHighlightedNode(nextItem);
            return;
        }
        const isModifierKey = e.ctrlKey || e.altKey || e.metaKey;
        const isCharacterKey = e.key.length === 1;
        const isSpaceKey = e.key === kbd.SPACE;
        const candidateNodes = this.root.getCandidateNodes();
        if (e.key === kbd.TAB)
            return;
        if (!isModifierKey && (isCharacterKey || isSpaceKey)) {
            const matchedNode = this.#domTypeahead.handleTypeaheadSearch(e.key, candidateNodes);
            if (isSpaceKey) {
                // prevent the button's activation `click`, which `onclick` would toggle
                e.preventDefault();
                if (!matchedNode)
                    this.#handleKeyboardSelection();
            }
            return;
        }
        if (!this.root.highlightedNode) {
            this.root.setHighlightedToFirstCandidate();
        }
    }
    onclick(e) {
        // While browsers generally have no issue focusing the trigger when clicking
        // on a label, Safari seems to struggle with the fact that there's no `onClick`.
        // We force `focus` in this case. Note: this doesn't create any other side-effect
        // because we are preventing default in `onpointerdown` so effectively
        // this only runs for a label 'click'
        const currTarget = e.currentTarget;
        currTarget.focus();
        // Screen readers can activate the trigger with a `click` that no pointer events
        // precede (VoiceOver in Safari). A click that ends a press has a non-zero `detail`
        // and was already handled by `onpointerdown` / `onpointerup`.
        if (this.root.opts.disabled.current || e.detail !== 0)
            return;
        if (this.root.opts.open.current) {
            this.root.handleClose();
        }
        else {
            this.#handleOpen();
        }
        e.preventDefault();
    }
    onpointerdown(e) {
        if (this.root.opts.disabled.current || isDisabledElement(this.opts.ref.current))
            return;
        // prevent opening on touch down which can be triggered when scrolling on touch devices
        if (e.pointerType === "touch")
            return e.preventDefault();
        // prevent implicit pointer capture
        const target = e.target;
        if (target?.hasPointerCapture(e.pointerId)) {
            target?.releasePointerCapture(e.pointerId);
        }
        // only call the handle if it's a left click, since pointerdown is triggered
        // by right clicks as well, but not when ctrl is pressed
        if (e.button === 0 && e.ctrlKey === false) {
            if (this.root.opts.open.current === false) {
                this.#handlePointerOpen(e);
            }
            else {
                this.root.handleClose();
            }
        }
    }
    onpointerup(e) {
        if (this.root.opts.disabled.current || isDisabledElement(this.opts.ref.current))
            return;
        e.preventDefault();
        if (e.pointerType === "touch") {
            if (this.root.opts.open.current === false) {
                this.#handlePointerOpen(e);
            }
            else {
                this.root.handleClose();
            }
        }
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        role: "combobox",
        disabled: this.root.opts.disabled.current ? true : undefined,
        "aria-haspopup": "listbox",
        "aria-expanded": boolToStr(this.root.opts.open.current),
        "aria-controls": this.root.contentNode?.id || undefined,
        "aria-activedescendant": this.root.highlightedId,
        "data-state": getDataOpenClosed(this.root.opts.open.current),
        "data-disabled": boolToEmptyStrOrUndef(this.root.opts.disabled.current),
        "data-placeholder": this.root.hasValue ? undefined : "",
        [this.root.getBitsAttr("trigger")]: "",
        onpointerdown: this.onpointerdown,
        onkeydown: this.onkeydown,
        onclick: this.onclick,
        onpointerup: this.onpointerup,
        ...this.attachment,
    }));
}
export class SelectContentState {
    static create(opts) {
        return SelectContentContext.set(new SelectContentState(opts, SelectRootContext.get()));
    }
    opts;
    root;
    attachment;
    isPositioned = $state(false);
    // set when the user scrolls the viewport by hand (wheel, touch, or holding a
    // scroll button) and reset on close; ends the realignment below
    userHasScrolled = false;
    domContext;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.attachment = attachRef(opts.ref, (v) => (this.root.contentNode = v));
        this.domContext = new DOMContext(this.opts.ref);
        if (this.root.domContext === null) {
            this.root.domContext = this.domContext;
        }
        onDestroyEffect(() => {
            this.root.contentNode = null;
            this.root.contentIsPositioned = false;
            this.isPositioned = false;
        });
        watch(() => this.root.opts.open.current, () => {
            if (this.root.opts.open.current)
                return;
            this.root.contentIsPositioned = false;
            this.isPositioned = false;
            this.userHasScrolled = false;
        });
        watch([() => this.isPositioned, () => this.root.highlightedNode], () => {
            if (!this.isPositioned || !this.root.highlightedNode)
                return;
            // a keyboard highlight hands the scroll position back to us
            if (this.root.isUsingKeyboard)
                this.userHasScrolled = false;
            this.root.scrollHighlightedNodeIntoView(this.root.highlightedNode);
        });
        // positioning is not the end of settling: chrome that mounts once the content
        // is placed (scroll buttons in the flex flow, a header, a footer) shrinks the
        // viewport under the alignment above and drops the highlighted item back below
        // the fold, so keep aligning on every resize until the user takes the scroll
        // position over
        watch([() => this.isPositioned, () => this.root.viewportNode], () => {
            const viewport = this.root.viewportNode;
            if (!this.isPositioned || !viewport)
                return;
            const onUserScroll = () => {
                this.userHasScrolled = true;
            };
            const observer = new (this.domContext.getWindow().ResizeObserver)(() => {
                if (this.userHasScrolled || !this.root.highlightedNode)
                    return;
                this.root.scrollHighlightedNodeIntoView(this.root.highlightedNode);
            });
            observer.observe(viewport);
            // not `scroll`: the realign scrolls the viewport itself, so `scroll` fires
            // for our own writes and cannot tell the user's gesture from ours
            return executeCallbacks(on(viewport, "wheel", onUserScroll, { passive: true }), on(viewport, "touchmove", onUserScroll, { passive: true }), () => observer.disconnect());
        });
        this.onpointermove = this.onpointermove.bind(this);
    }
    onpointermove(_) {
        this.root.isUsingKeyboard = false;
    }
    // every press starts fresh, so a press that ended off an item isn't reused
    onpointerdowncapture = () => {
        this.root.pressedItem = null;
    };
    #styles = $derived.by(() => {
        return getFloatingContentCSSVars(this.root.isCombobox ? "combobox" : "select");
    });
    onInteractOutside = (e) => {
        if (e.target === this.root.triggerNode || e.target === this.root.inputNode) {
            e.preventDefault();
            return;
        }
        this.opts.onInteractOutside.current(e);
        if (e.defaultPrevented)
            return;
        this.root.handleClose();
    };
    onEscapeKeydown = (e) => {
        this.opts.onEscapeKeydown.current(e);
        if (e.defaultPrevented)
            return;
        this.root.handleClose();
    };
    onOpenAutoFocus = (e) => {
        e.preventDefault();
    };
    onCloseAutoFocus = (e) => {
        e.preventDefault();
    };
    get shouldRender() {
        return this.root.contentPresence.shouldRender;
    }
    snippetProps = $derived.by(() => ({ open: this.root.opts.open.current }));
    props = $derived.by(() => ({
        id: this.opts.id.current,
        role: "listbox",
        "aria-multiselectable": this.root.isMulti ? "true" : undefined,
        "data-state": getDataOpenClosed(this.root.opts.open.current),
        ...getDataTransitionAttrs(this.root.contentPresence.transitionStatus),
        [this.root.getBitsAttr("content")]: "",
        style: {
            display: "flex",
            flexDirection: "column",
            outline: "none",
            boxSizing: "border-box",
            pointerEvents: "auto",
            ...this.#styles,
        },
        onpointermove: this.onpointermove,
        onpointerdowncapture: this.onpointerdowncapture,
        ...this.attachment,
    }));
    popperProps = {
        onInteractOutside: this.onInteractOutside,
        onEscapeKeydown: this.onEscapeKeydown,
        onOpenAutoFocus: this.onOpenAutoFocus,
        onCloseAutoFocus: this.onCloseAutoFocus,
        trapFocus: false,
        loop: false,
        onPlaced: () => {
            // onPlaced is also called when the menu is closed, so we need to check if the menu
            // is actually open to avoid setting positioning to true when the menu is closed
            if (this.root.opts.open.current) {
                this.root.contentIsPositioned = true;
                this.isPositioned = true;
            }
        },
    };
}
export class SelectItemState {
    static create(opts) {
        return new SelectItemState(opts, SelectRootContext.get());
    }
    opts;
    root;
    attachment;
    isSelected = $derived.by(() => this.root.includesItem(this.opts.value.current));
    isHighlighted = $derived.by(() => this.root.highlightedValue === this.opts.value.current);
    /**
     * Kept as a separate boolean derived so that `props` (and the `mergeProps` /
     * attribute-diffing work downstream of it) is only invalidated for the two items whose
     * highlighted state actually changed, rather than for every mounted item each time the
     * highlighted value moves.
     */
    #isHighlightedAndEnabled = $derived.by(() => this.isHighlighted && !this.opts.disabled.current);
    prevHighlighted = new Previous(() => this.isHighlighted);
    mounted = $state(false);
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.attachment = attachRef(opts.ref);
        watch([() => this.isHighlighted, () => this.prevHighlighted.current], () => {
            if (this.isHighlighted) {
                this.opts.onHighlight.current();
            }
            else if (this.prevHighlighted.current) {
                this.opts.onUnhighlight.current();
            }
        });
        watch(() => this.mounted, () => {
            if (!this.mounted)
                return;
            this.root.setInitialHighlightedNode();
        });
        // keep the cached label in sync while this item is selected and mounted
        watch([
            () => this.isSelected,
            () => this.mounted,
            () => this.opts.value.current,
            () => this.opts.label.current,
        ], ([selected, mounted, value]) => {
            if (!selected || !mounted)
                return;
            this.root.setSelectedLabel(value, this.#resolveLabel());
        });
        this.onpointerdown = this.onpointerdown.bind(this);
        this.onpointerup = this.onpointerup.bind(this);
        this.onpointermove = this.onpointermove.bind(this);
    }
    #resolveLabel() {
        return this.root.resolveItemLabel(this.opts.value.current, this.opts.label.current, this.opts.ref.current);
    }
    handleSelect() {
        if (this.opts.disabled.current)
            return;
        const isCurrentSelectedValue = this.opts.value.current === this.root.opts.value.current;
        // if allowDeselect is false and the item is already selected and we're not in a
        // multi select, do nothing and close the menu
        if (!this.root.opts.allowDeselect.current && isCurrentSelectedValue && !this.root.isMulti) {
            this.root.handleClose();
            return;
        }
        // otherwise, toggle the item and if we're not in a multi select, close the menu
        this.root.toggleItem(this.opts.value.current, this.#resolveLabel());
        if (!this.root.isMulti && !isCurrentSelectedValue) {
            this.root.handleClose();
        }
    }
    snippetProps = $derived.by(() => ({
        selected: this.isSelected,
        highlighted: this.isHighlighted,
    }));
    onpointerdown(e) {
        // prevent focus from leaving the input/select trigger
        e.preventDefault();
        this.root.pressedItem = { item: this, x: e.clientX, y: e.clientY };
    }
    /**
     * Using `pointerup` instead of `click` allows power users to pointerdown
     * the trigger, then release pointerup on an item to select it vs having to do
     * multiple clicks.
     */
    onpointerup(e) {
        if (e.defaultPrevented || !this.opts.ref.current)
            return;
        const press = this.root.pressedItem;
        this.root.pressedItem = null;
        // released on another item without the pointer moving: the list moved under it
        if (press &&
            press.item !== this &&
            Math.hypot(e.clientX - press.x, e.clientY - press.y) < 4) {
            e.preventDefault();
            press.item.handleSelect();
            return;
        }
        /**
         * For one reason or another, when it's a touch pointer and _not_ on IOS,
         * we need to listen for the immediate click event to handle the selection,
         * otherwise a click event will fire on the element _behind_ the item.
         */
        if (e.pointerType === "touch" && !isIOS) {
            on(this.opts.ref.current, "click", () => {
                this.handleSelect();
                // set highlighted node since we don't do it on `pointermove` events
                // for touch devices
                this.root.setHighlightedNode(this.opts.ref.current);
            }, { once: true });
            return;
        }
        e.preventDefault();
        this.handleSelect();
        if (e.pointerType === "touch") {
            // set highlighted node since we don't do it on `pointermove` events
            // for touch devices
            this.root.setHighlightedNode(this.opts.ref.current);
        }
    }
    onpointermove(e) {
        /**
         * We don't want to highlight items on touch devices when scrolling,
         * as this is confusing behavior, so we return here and instead handle
         * the highlighting on the `pointerup` (or following `click`) event for
         * touch devices only.
         */
        if (e.pointerType === "touch")
            return;
        if (this.root.highlightedNode !== this.opts.ref.current) {
            this.root.setHighlightedNode(this.opts.ref.current);
        }
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        role: "option",
        "aria-selected": boolToStr(this.isSelected),
        "data-value": this.opts.value.current,
        "data-disabled": boolToEmptyStrOrUndef(this.opts.disabled.current),
        "data-highlighted": boolToEmptyStrOrUndef(this.#isHighlightedAndEnabled),
        "data-selected": boolToEmptyStrOrUndef(this.isSelected),
        "data-label": this.opts.label.current,
        [this.root.getBitsAttr("item")]: "",
        onpointermove: this.onpointermove,
        onpointerdown: this.onpointerdown,
        onpointerup: this.onpointerup,
        ...this.attachment,
    }));
}
export class SelectGroupState {
    static create(opts) {
        return SelectGroupContext.set(new SelectGroupState(opts, SelectRootContext.get()));
    }
    opts;
    root;
    labelNode = $state(null);
    attachment;
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.attachment = attachRef(opts.ref);
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        role: "group",
        [this.root.getBitsAttr("group")]: "",
        "aria-labelledby": this.labelNode?.id ?? undefined,
        ...this.attachment,
    }));
}
export class SelectGroupHeadingState {
    static create(opts) {
        return new SelectGroupHeadingState(opts, SelectGroupContext.get());
    }
    opts;
    group;
    attachment;
    constructor(opts, group) {
        this.opts = opts;
        this.group = group;
        this.attachment = attachRef(opts.ref, (v) => (this.group.labelNode = v));
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        [this.group.root.getBitsAttr("group-label")]: "",
        ...this.attachment,
    }));
}
export class SelectHiddenInputState {
    static create(opts) {
        return new SelectHiddenInputState(opts, SelectRootContext.get());
    }
    opts;
    root;
    shouldRender = $derived.by(() => this.root.opts.name.current !== "");
    constructor(opts, root) {
        this.opts = opts;
        this.root = root;
        this.onfocus = this.onfocus.bind(this);
    }
    onfocus(e) {
        e.preventDefault();
        if (!this.root.isCombobox) {
            this.root.triggerNode?.focus();
        }
        else {
            this.root.inputNode?.focus();
        }
    }
    props = $derived.by(() => ({
        disabled: boolToTrueOrUndef(this.root.opts.disabled.current),
        required: boolToTrueOrUndef(this.root.opts.required.current),
        name: this.root.opts.name.current,
        value: this.opts.value.current,
        onfocus: this.onfocus,
    }));
}
export class SelectViewportState {
    static create(opts) {
        return new SelectViewportState(opts, SelectContentContext.get());
    }
    opts;
    content;
    root;
    attachment;
    prevScrollTop = $state(0);
    constructor(opts, content) {
        this.opts = opts;
        this.content = content;
        this.root = content.root;
        this.attachment = attachRef(opts.ref, (v) => {
            this.root.viewportNode = v;
        });
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        role: "presentation",
        [this.root.getBitsAttr("viewport")]: "",
        style: {
            // we use position: 'relative' here on the `viewport` so that when we call
            // `selectedItem.offsetTop` in calculations, the offset is relative to the viewport
            // (independent of the scrollUpButton).
            position: "relative",
            flex: 1,
            overflow: "auto",
        },
        ...this.attachment,
    }));
}
export class SelectScrollButtonImplState {
    opts;
    content;
    root;
    attachment;
    autoScrollTimer = null;
    userScrollTimer = -1;
    isUserScrolling = false;
    onAutoScroll = noop;
    mounted = $state(false);
    constructor(opts, content) {
        this.opts = opts;
        this.content = content;
        this.root = content.root;
        this.attachment = attachRef(opts.ref);
        watch([() => this.mounted], () => {
            if (this.mounted)
                return;
            this.isUserScrolling = false;
        });
        $effect(() => {
            if (this.mounted && this.root.opts.open.current)
                return;
            this.clearAutoScrollInterval();
        });
        // closing destroys the button together with the effect above, so the effect never
        // sees `mounted` flip and a removed button gets no `pointerleave`. without this the
        // timer outlives the button and scrolls the viewport the next time the content opens
        onDestroyEffect(() => this.clearAutoScrollInterval());
        this.onpointerdown = this.onpointerdown.bind(this);
        this.onpointermove = this.onpointermove.bind(this);
        this.onpointerleave = this.onpointerleave.bind(this);
    }
    handleUserScroll() {
        this.content.domContext.clearTimeout(this.userScrollTimer);
        this.isUserScrolling = true;
        this.userScrollTimer = this.content.domContext.setTimeout(() => {
            this.isUserScrolling = false;
        }, 200);
    }
    clearAutoScrollInterval() {
        if (this.autoScrollTimer === null)
            return;
        this.content.domContext.clearTimeout(this.autoScrollTimer);
        this.autoScrollTimer = null;
    }
    onpointerdown(_) {
        if (this.autoScrollTimer !== null || !this.root.opts.open.current)
            return;
        this.content.userHasScrolled = true;
        const autoScroll = (tick) => {
            this.onAutoScroll();
            this.autoScrollTimer = this.content.domContext.setTimeout(() => autoScroll(tick + 1), this.opts.delay.current(tick));
        };
        this.autoScrollTimer = this.content.domContext.setTimeout(() => autoScroll(1), this.opts.delay.current(0));
    }
    onpointermove(e) {
        this.onpointerdown(e);
    }
    onpointerleave(_) {
        this.clearAutoScrollInterval();
    }
    props = $derived.by(() => ({
        id: this.opts.id.current,
        "aria-hidden": boolToStrTrueOrUndef(true),
        style: {
            flexShrink: 0,
        },
        onpointerdown: this.onpointerdown,
        onpointermove: this.onpointermove,
        onpointerleave: this.onpointerleave,
        ...this.attachment,
    }));
}
export class SelectScrollDownButtonState {
    static create(opts) {
        return new SelectScrollDownButtonState(new SelectScrollButtonImplState(opts, SelectContentContext.get()));
    }
    scrollButtonState;
    content;
    root;
    canScrollDown = $state(false);
    constructor(scrollButtonState) {
        this.scrollButtonState = scrollButtonState;
        this.content = scrollButtonState.content;
        this.root = scrollButtonState.root;
        this.scrollButtonState.onAutoScroll = this.handleAutoScroll;
        watch([() => this.root.viewportNode, () => this.content.isPositioned], () => {
            const viewport = this.root.viewportNode;
            if (!viewport || !this.content.isPositioned)
                return;
            this.handleScroll(true);
            return on(viewport, "scroll", () => this.handleScroll());
        });
        /**
         * If the input value changes, this means that the filtered items may have changed,
         * so we need to re-evaluate the scroll-ability of the list.
         */
        watch([
            () => this.root.opts.inputValue.current,
            () => this.root.viewportNode,
            () => this.content.isPositioned,
        ], () => {
            if (!this.root.viewportNode || !this.content.isPositioned)
                return;
            this.handleScroll(true);
        });
    }
    /**
     * @param manual - if true, it means the function was invoked manually outside of an event
     * listener, so we don't call `handleUserScroll` to prevent the auto scroll from kicking in.
     */
    handleScroll = (manual = false) => {
        if (!manual) {
            this.scrollButtonState.handleUserScroll();
        }
        if (!this.root.viewportNode)
            return;
        const maxScroll = this.root.viewportNode.scrollHeight - this.root.viewportNode.clientHeight;
        const paddingTop = Number.parseInt(getComputedStyle(this.root.viewportNode).paddingTop, 10);
        this.canScrollDown = Math.ceil(this.root.viewportNode.scrollTop) < maxScroll - paddingTop;
    };
    handleAutoScroll = () => {
        const viewport = this.root.viewportNode;
        const selectedItem = this.root.highlightedNode;
        if (!viewport || !selectedItem)
            return;
        viewport.scrollTop = viewport.scrollTop + selectedItem.offsetHeight;
    };
    props = $derived.by(() => ({
        ...this.scrollButtonState.props,
        [this.root.getBitsAttr("scroll-down-button")]: "",
    }));
}
export class SelectScrollUpButtonState {
    static create(opts) {
        return new SelectScrollUpButtonState(new SelectScrollButtonImplState(opts, SelectContentContext.get()));
    }
    scrollButtonState;
    content;
    root;
    canScrollUp = $state(false);
    constructor(scrollButtonState) {
        this.scrollButtonState = scrollButtonState;
        this.content = scrollButtonState.content;
        this.root = scrollButtonState.root;
        this.scrollButtonState.onAutoScroll = this.handleAutoScroll;
        watch([() => this.root.viewportNode, () => this.content.isPositioned], () => {
            if (!this.root.viewportNode || !this.content.isPositioned)
                return;
            this.handleScroll(true);
            return on(this.root.viewportNode, "scroll", () => this.handleScroll());
        });
    }
    /**
     * @param manual - if true, it means the function was invoked manually outside of an event
     * listener, so we don't call `handleUserScroll` to prevent the auto scroll from kicking in.
     */
    handleScroll = (manual = false) => {
        if (!manual) {
            this.scrollButtonState.handleUserScroll();
        }
        if (!this.root.viewportNode)
            return;
        const paddingTop = Number.parseInt(getComputedStyle(this.root.viewportNode).paddingTop, 10);
        this.canScrollUp = this.root.viewportNode.scrollTop - paddingTop > 0.1;
    };
    handleAutoScroll = () => {
        if (!this.root.viewportNode || !this.root.highlightedNode)
            return;
        this.root.viewportNode.scrollTop =
            this.root.viewportNode.scrollTop - this.root.highlightedNode.offsetHeight;
    };
    props = $derived.by(() => ({
        ...this.scrollButtonState.props,
        [this.root.getBitsAttr("scroll-up-button")]: "",
    }));
}
