<script lang="ts">
	/**
	 * Ein Satz wie „Anstehendes siehst du in Microsoft Edge unter
	 * http://localhost:8787“ (ohne Anmeldung, Etappe 3b, Fund des Prüfers):
	 * Die Adresse am Ende wird ein Link auf dieselbe Seite unter localhost,
	 * dort meldet Windows von selbst an. Andere Sätze stehen unverändert da.
	 */
	import { BROWSER_ADRESSE, localhostAdresse } from '#lib/anmeldung.ts';
	import OldLink from './OldLink.svelte';

	interface Props {
		satz: string;
		class?: string;
	}
	let { satz, class: cls }: Props = $props();

	const vorn = $derived(
		satz.endsWith(BROWSER_ADRESSE) ? satz.slice(0, -BROWSER_ADRESSE.length) : null
	);
	// Dieselbe Seite unter localhost; ohne Fenster (Tests) die Startadresse.
	const ziel = typeof location === 'undefined' ? BROWSER_ADRESSE : localhostAdresse(location.href);
</script>

<p class={cls}>
	{#if vorn !== null}{vorn}<OldLink href={ziel}>{BROWSER_ADRESSE}</OldLink>.{:else}{satz}{/if}
</p>
