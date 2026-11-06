<script lang="ts">
	/**
	 * Ein Foto oder Bild aus `assets/` des eigenen Projekts:
	 *
	 *   <Bild datei="foto.jpg" alt="Blick auf das Werk im Sommer" beschriftung="Das Werk von außen" breite="klein" />
	 *
	 * `datei` ist nur der Dateiname (kein Pfad, keine Adresse), `alt` sagt
	 * Bildschirmlesern, was zu sehen ist. Das Projekt kommt aus der Route wie
	 * bei `ladeDaten`. Ein falscher Name zeigt einen Satz statt des Bildes.
	 */
	import { kontext } from '../projekt/laden.svelte.ts';
	import { bildAdresse, bildFehler } from './bild.ts';
	import { assetStand, mitVersion } from '../assets.svelte.ts';

	interface Props {
		/** Dateiname in `assets/`, etwa `foto.jpg`. */
		datei: string;
		/** Was auf dem Bild zu sehen ist (für Bildschirmleser), Pflicht. */
		alt: string;
		/** Überschrift unter dem Bild. */
		titel?: string | undefined;
		/** Satz unter dem Bild (Quelle, Ort, Zeit). */
		beschriftung?: string | undefined;
		/** Wie breit höchstens: `klein`, `mittel` oder `voll` (so breit wie die Spalte). */
		breite?: 'klein' | 'mittel' | 'voll' | undefined;
	}
	let { datei, alt, titel, beschriftung, breite = 'voll' }: Props = $props();

	const projekt = kontext('Bild');

	const fehler = $derived(
		bildFehler(datei) ??
			(alt.trim() === ''
				? 'Bild braucht `alt`: einen kurzen Satz, was auf dem Bild zu sehen ist (für Bildschirmleser).'
				: null)
	);
	// Live-Lücken: nach einer Änderung dieser Datei in assets/ mit neuer Version
	// (`?v=`), also neu geholt; andere geänderte Dateien lassen das Bild in Ruhe.
	const adresse = $derived(
		mitVersion(bildAdresse(projekt.slug, datei), assetStand.version(projekt.slug, datei))
	);
	/** Die Datei kam nicht (fehlt in assets/ oder zu groß). */
	let kaputt = $state<string | null>(null);
	const unterschrift = $derived(Boolean(titel) || Boolean(beschriftung));
</script>

{#if fehler !== null}
	<p class="block-error bild-fehler" role="alert">{fehler}</p>
{:else if kaputt === adresse}
	<p class="block-error bild-fehler" role="alert">
		Das Bild „{datei}“ lässt sich nicht laden. Liegt die Datei in assets/ und ist sie kleiner als 2
		MB?
	</p>
{:else}
	<figure class="bild breite-{breite}">
		<img
			src={adresse}
			{alt}
			loading="lazy"
			decoding="async"
			onerror={() => {
				kaputt = adresse;
			}}
		/>
		{#if unterschrift}
			<figcaption>
				{#if titel}<strong>{titel}</strong>{/if}
				{#if beschriftung}<span class="muted">{beschriftung}</span>{/if}
			</figcaption>
		{/if}
	</figure>
{/if}

<style>
	.bild {
		margin: 0.6em 0 1em;
		max-width: 100%;
	}
	.breite-klein {
		max-width: min(16rem, 100%);
	}
	.breite-mittel {
		max-width: min(32rem, 100%);
	}
	img {
		display: block;
		max-width: 100%;
		height: auto;
		border-radius: var(--radius);
		border: 1px solid var(--line);
	}
	.breite-klein img,
	.breite-mittel img {
		width: 100%;
	}
	figcaption {
		display: flex;
		flex-direction: column;
		gap: 0.15em;
		margin-top: 0.4em;
		font-size: var(--text-em-md);
	}
	.bild-fehler {
		margin: 0.6em 0 1em;
		overflow-wrap: anywhere;
	}
</style>
