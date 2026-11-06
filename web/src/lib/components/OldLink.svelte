<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * Link nach außerhalb der App, etwa auf eine fremde Adresse oder einen
	 * Ordner. Kein resolve(), weil SvelteKit diese Ziele nicht kennt;
	 * `external` öffnet in neuem Tab.
	 */
	interface Props {
		href: string;
		title?: string;
		class?: string;
		external?: boolean;
		/** Dateiname: der Browser lädt das Ziel herunter, statt es zu zeigen. */
		download?: string;
		children: Snippet;
	}
	let { href, title, class: cls, external = false, download, children }: Props = $props();
</script>

<!-- eslint-disable svelte/no-navigation-without-resolve -- Ziel liegt außerhalb der App, resolve() kennt es nicht -->
<a
	{href}
	{title}
	class={cls}
	{download}
	data-sveltekit-reload
	target={external ? '_blank' : undefined}
	rel={external ? 'noopener' : undefined}>{@render children()}</a
>
<!-- eslint-enable svelte/no-navigation-without-resolve -->
