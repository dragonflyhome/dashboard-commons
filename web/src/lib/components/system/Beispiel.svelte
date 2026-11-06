<script lang="ts">
	/**
	 * Ein `beispiel` aus einem Block einer System-Seite: vorformatiert in
	 * Monospace, die Zeilen bleiben, wie sie in der Datei stehen. Reiner
	 * Text, nie HTML. Breite Zeilen rollen im eigenen Rahmen seitlich (die
	 * Seite wird nie breiter); nur dann ist der Rahmen ein benannter Bereich
	 * mit Tab-Stopp, wie bei der Tabelle.
	 */
	import { scrollShadow } from '#lib/scrollschatten.ts';

	interface Props {
		text: string;
		/** Name des Bereichs für Vorlese-Programme, wenn er rollt. */
		name: string;
	}
	let { text, name }: Props = $props();

	// Windows-Zeilenenden werden \n.
	const zeilen = $derived(text.replaceAll('\r\n', '\n'));

	let rahmen: HTMLPreElement | undefined = $state();
	let rollt = $state(false);
	/** Leerer Text rollt nie; sonst zählt, ob der Inhalt breiter als der Rahmen ist. */
	const messen = (el: HTMLElement, text: string): void => {
		rollt = text !== '' && el.scrollWidth > el.clientWidth;
	};
	// Größe des Rahmens: der ResizeObserver.
	$effect(() => {
		const el = rahmen;
		if (!el || typeof ResizeObserver === 'undefined') return;
		const ro = new ResizeObserver(() => {
			messen(el, zeilen);
		});
		ro.observe(el);
		return () => {
			ro.disconnect();
		};
	});
	// Neuer Text (etwa nach einem Live-Neuladen) bei gleich großem Rahmen:
	// der ResizeObserver merkt das nicht, darum hier neu messen.
	$effect(() => {
		const el = rahmen;
		if (el) messen(el, zeilen);
	});
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (rollt der Rahmen, ist er nach WCAG 2.1.1 per Tab erreichbar und benannt) -->
<pre
	bind:this={rahmen}
	class="scroll-x beispiel"
	role={rollt ? 'region' : undefined}
	aria-label={rollt ? name : undefined}
	tabindex={rollt ? 0 : undefined}
	use:scrollShadow>{zeilen}</pre>

<style>
	.beispiel {
		/* Die Schatten von .scroll-x liegen als Bilder über der Fläche des Codes. */
		background-color: var(--code-bg);
		font-family: var(--mono);
		font-size: var(--text-sm);
		white-space: pre;
		overflow-wrap: normal;
		overflow-x: auto;
	}
</style>
