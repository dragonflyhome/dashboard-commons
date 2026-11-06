<script lang="ts">
	// Kommentar mit fetch('/x'), 2026-11-02 und [3, 5, 8] zählt nicht.
	/* localStorage hier auch nicht */
	import { Karte } from '$katalog';
	import { ladeDaten } from '$projekt';
	import type { Fortschritt } from '$vertrag';
	import type { Plan } from '$vertrag/projekte/gut';
	import { fade } from 'svelte/transition';
	import Teil from './Teil.svelte';
	import type { Zeile } from './typen';

	const daten = ladeDaten<Fortschritt>('gut', 'fortschritt');
	let offen = $state(0);
	const davor = (n: number) => n - 1;
	const plan: Plan | undefined = undefined;
	const zeilen: Zeile[] = [];
</script>

<!-- Kommentar: fetch, {@html x}, #ff0000, 2026-11-02 -->

<Karte titel="Plan">
	{#each zeilen as zeile, i (zeile.name)}
		<p transition:fade>{i + 1}. {zeile.name}</p>
	{/each}
	<a href="https://example.org">Hersteller</a>
	<a href="/projects/gut">Zurück</a>
	<button onclick={() => (offen = offen + 1)}>Mehr</button>
	<Teil {plan} {daten} />
	<span tabindex="-1">{davor(offen)}</span>
</Karte>

<style>
	p {
		color: var(--text);
		margin: 0 0 12px;
		border: 1px solid var(--line);
		grid-template-columns: 1fr 2fr 3fr;
	}
	@media (min-width: 40rem) {
		p:hover {
			background: var(--accent-soft);
		}
	}
</style>
