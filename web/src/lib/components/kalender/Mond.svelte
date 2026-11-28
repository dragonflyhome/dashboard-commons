<script lang="ts">
	/**
	 * Kalender, Stufe 7b: der Mond in seiner Phase, selbst gezeichnet (Pfad aus
	 * `mondPfad`), auf der Südhalbkugel gespiegelt. Ohne `name` nur Schmuck
	 * (`aria-hidden`, so in der Kopfleiste, deren Link den Satz trägt); mit `name`
	 * ein Bild mit Namen und Textfassung (Phase und nächster Vollmond), so auf /heute.
	 */
	import { mondPfad } from '#lib/himmel.svelte.ts';
	import { mondText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';
	import type { KalenderKopf } from '$vertrag';

	interface Props {
		kopf: Pick<KalenderKopf, 'phase_grad' | 'phase' | 'sued' | 'naechster_vollmond'>;
		/** Kantenlänge in Pixeln. */
		groesse?: number;
		/** Name des Bilds; ohne ihn ist der Mond nur Schmuck. */
		name?: string | undefined;
	}
	let { kopf, groesse = 22, name }: Props = $props();

	const mond = $derived(mondPfad(kopf.phase_grad, 9));
</script>

<svg
	class="mond"
	viewBox="-10 -10 20 20"
	width={groesse}
	height={groesse}
	role={name === undefined ? undefined : 'img'}
	aria-label={name}
	aria-hidden={name === undefined ? 'true' : undefined}
>
	<circle class="dunkel" r="9" />
	<path
		class="hell"
		d={mond.d}
		transform={mond.spiegeln !== kopf.sued ? 'scale(-1 1)' : undefined}
	/>
	<circle class="rand" r="9" />
</svg>
{#if name !== undefined}
	<Textfassung fassung={mondText(kopf)} />
{/if}

<style>
	.mond {
		flex: none;
	}
	/* Der beleuchtete Teil ist in beiden Themen heller als der Schatten (über 3:1,
	   kontrast.spec.ts): hell --flaeche auf --vordergrund, dunkel --vordergrund auf --linie.
	   light-dark() folgt color-scheme, das app.css für System und Wahl „Hell“/„Dunkel“ setzt. */
	.dunkel {
		fill: light-dark(var(--vordergrund), var(--linie));
	}
	.hell {
		fill: light-dark(var(--flaeche), var(--vordergrund));
	}
	/* Der Rand obenauf hält Neu- und Vollmond sichtbar (--vordergrund-leise gegen --flaeche
	   und --grundfarbe-soft über 3:1); er bleibt bei jeder Größe einen Pixel breit. */
	.rand {
		fill: none;
		stroke: var(--vordergrund-leise);
		stroke-width: 1;
		vector-effect: non-scaling-stroke;
	}
</style>
