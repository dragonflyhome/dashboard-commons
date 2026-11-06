<script lang="ts">
	/**
	 * Rückmeldung nach dem Beantworten einer Frage (Etappe A2): „Gesendet –
	 * der Chat ist geweckt“ oder ein Fehler in einfachem Deutsch. Der Bereich
	 * steht immer im Dokument (auch leer, dann nur optisch nicht zu sehen),
	 * damit Vorlese-Programme die Meldung ansagen, sobald sie erscheint. Er ist
	 * nur für die Ansage da.
	 *
	 * Fokus: Die beantwortete Frage verschwindet beim Neuladen samt dem Knopf,
	 * auf dem der Fokus lag; ohne Hilfe landet er dann auf `body`. Darum setzt
	 * der Baustein den Fokus einmal je neuer Meldung auf `ziel`: ein Element
	 * der Seite außerhalb dieses Bereichs (Überschrift oder Hülle des
	 * Abschnitts mit `tabindex="-1"`), das stehen bleibt, auch wenn die Meldung
	 * nach 15 s verschwindet oder die letzte Frage weg ist. Läge der Fokus auf
	 * der Meldung selbst, käme sie oft doppelt, und nach 15 s fiele er auf
	 * `body`.
	 *
	 * Seit A3 (Auflage aus A2) nimmt er den Fokus nur für eine Antwort aus
	 * seinem eigenen `abschnitt` und nur, solange der Fokus noch auf `body`
	 * oder dem gesperrten Knopf liegt; sonst bleibt er, wo er ist.
	 */
	import { antworten, type Antworten } from '#lib/antworten.svelte.ts';

	interface Props {
		/** Nur Meldungen zu diesem Projekt; `null`: alle. */
		projekt?: string | null;
		/** Dorthin geht der Fokus bei einer neuen Meldung (außerhalb des Bereichs). */
		ziel: HTMLElement | null;
		/** Der Abschnitt, zu dem dieser Bereich gehört (wie bei `FrageAntwort`). */
		abschnitt?: string;
		speicher?: Antworten;
	}
	let { projekt = null, ziel, abschnitt = '', speicher = antworten }: Props = $props();

	const m = $derived(speicher.meldungFuer(projekt));

	$effect(() => {
		if (m !== null && ziel !== null && speicher.fokusNehmen(m, abschnitt)) ziel.focus();
	});
</script>

<div class="antwort-status" aria-live="polite" aria-atomic="true">
	{#if m}
		<p class:gut={m.art === 'gesendet'} class:block-error={m.art === 'fehler'}>
			{#if m.art === 'gesendet'}<span aria-hidden="true">✓ </span>{/if}{m.satz}
			{#if m.art === 'gesendet' && m.eigene}
				<!-- „Etwas anderes …“: die eigenen Worte stehen da, wie sonst die geklickte Antwort. -->
				<span class="eigene">Deine Antwort: {m.eigene}</span>
			{/if}
		</p>
	{/if}
</div>

<style>
	p {
		margin: 0.4rem 0;
		overflow-wrap: anywhere;
	}
	.eigene {
		display: block;
		margin-top: 0.25rem;
		white-space: pre-wrap;
	}
	.gut {
		padding: 0.35rem 0.7rem;
		border-left: 0.25rem solid var(--ok);
		background: color-mix(in srgb, var(--ok) 12%, transparent);
		border-radius: 0.4rem;
	}
</style>
