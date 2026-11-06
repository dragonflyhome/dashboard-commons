<script lang="ts">
	import { untrack } from "svelte";
	import type { ScrollLockProps } from "./index.js";
	import { BodyScrollLock } from "../../../internal/body-scroll-lock.svelte.js";

	let { preventScroll = true, restoreScrollDelay = null }: ScrollLockProps = $props();

	untrack(() => {
		if (preventScroll) {
			new BodyScrollLock(preventScroll, () => restoreScrollDelay);
		}
	});
</script>
