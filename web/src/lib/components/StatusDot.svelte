<script lang="ts">
	import type { Bedeutung } from '$vertrag';
	import { klasse, wort } from '#lib/bedeutung.ts';

	interface Props {
		/** Was der Punkt bedeutet; die Farbe folgt daraus. */
		bedeutung: Bedeutung;
		/** Text für Screenreader; Standard: das Wort zur Bedeutung. */
		label?: string;
		/** Nur Schmuck neben einem Text, der den Status schon nennt. */
		decorative?: boolean;
	}
	let { bedeutung, label, decorative = false }: Props = $props();
</script>

{#if decorative}
	<span class="dot {klasse(bedeutung)}" aria-hidden="true"></span>
{:else}
	<span class="dot {klasse(bedeutung)}" role="img" aria-label={label ?? wort(bedeutung)}></span>
{/if}
