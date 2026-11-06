<script lang="ts">
	/**
	 * Eine Zeile der Vorgangsliste: Punkt nach Zustand, Art, Projekt, Titel,
	 * seit wann. Ein Klick klappt die Schritte auf; `laden` holt sie beim
	 * ersten Aufklappen (auf der Seite `api.vorgang`, im Test ein Ersatz).
	 */
	import { resolve } from '$app/paths';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { fehlerText } from '#lib/api/client.ts';
	import {
		artWort,
		fmtZeitpunkt,
		herkunftWort,
		istUeberfaellig,
		seitText,
		zustandBedeutung,
		zustandWort
	} from '#lib/vorgaenge.ts';
	import type { Ereignis, Vorgang, VorgangDetail } from '$vertrag';

	interface Props {
		vorgang: Vorgang;
		/** Holt den Vorgang mit allen Ereignissen (GET /api/vorgaenge/{id}). */
		laden: (id: string) => Promise<VorgangDetail>;
		/** Bezugszeit, für Tests; Standard: jetzt. */
		jetzt?: Date;
	}
	let { vorgang, laden, jetzt }: Props = $props();

	const now = $derived(jetzt ?? new Date());
	const rot = $derived(istUeberfaellig(vorgang, now));
	const panelId = $derived(`vorgang-${vorgang.id}`);

	let offen = $state(false);
	let laedt = $state(false);
	let ereignisse = $state<Ereignis[] | null>(null);
	let fehler = $state<string | null>(null);
	/** Bei neuen Ereignissen (Live) beim nächsten Aufklappen neu laden. */
	let geladenBis = $state(-1);

	async function umschalten(): Promise<void> {
		offen = !offen;
		if (!offen) return;
		await nachladen();
	}

	/** Holt die Schritte, wenn sie fehlen oder älter sind als die Kopfzeile. */
	async function nachladen(): Promise<void> {
		if (laedt) return;
		if (ereignisse !== null && geladenBis === vorgang.letzte_nr) return;
		laedt = true;
		fehler = null;
		// Der Stand der Kopfzeile beim Abruf; kommt währenddessen ein neuer, lädt der Effekt unten nach.
		const bis = vorgang.letzte_nr;
		try {
			const detail = await laden(vorgang.id);
			ereignisse = detail.ereignisse;
			geladenBis = bis;
		} catch (e) {
			fehler = fehlerText(e);
		} finally {
			laedt = false;
		}
	}

	// Live-Lücken: Aufgeklappt und die Kopfzeile ist neuer (die Liste kam nach einem
	// Ereignis `vorgang` neu), dann die Schritte gleich nachladen, nicht erst beim
	// nächsten Aufklappen.
	$effect(() => {
		if (offen && ereignisse !== null && fehler === null && vorgang.letzte_nr !== geladenBis)
			void nachladen();
	});
</script>

<li class="zeile" class:rot>
	<div class="kopf">
		<button
			type="button"
			class="auf"
			aria-expanded={offen}
			aria-controls={panelId}
			onclick={umschalten}
		>
			<StatusDot
				bedeutung={zustandBedeutung(vorgang.zustand)}
				label={zustandWort(vorgang.zustand)}
			/>
			<span class="pill art">{artWort(vorgang.art)}</span>
			<span class="titel">{vorgang.titel}</span>
			<span class="muted small seit">{seitText(vorgang, now)}</span>
		</button>
		<a class="projekt small" href={resolve('/projects/[slug]', { slug: vorgang.slug })}
			>{vorgang.slug}</a
		>
	</div>
	{#if rot}
		<p class="warnung small">Der Chat hat seit über einem Tag nicht reagiert.</p>
	{/if}
	<!-- Immer da (zu: `hidden`), damit `aria-controls` ein vorhandenes Element nennt. -->
	<div class="schritte" id={panelId} hidden={!offen}>
		{#if offen}
			{#if laedt}
				<p class="muted small">Lade die Schritte …</p>
			{:else if fehler}
				<p class="block-error">{fehler}</p>
			{:else if ereignisse !== null && ereignisse.length === 0}
				<p class="muted small">Noch keine Schritte.</p>
			{:else if ereignisse !== null}
				<ol class="plain">
					{#each ereignisse as e (e.nr)}
						<li>
							<span class="zeit muted small">{fmtZeitpunkt(e.zeit, now)}</span>
							<StatusDot bedeutung={zustandBedeutung(e.zustand)} decorative />
							<strong>{zustandWort(e.zustand)}</strong>
							<span class="muted">von {herkunftWort(e.von)}</span>
							{#if e.notiz}<span class="notiz">„{e.notiz}“</span>{/if}
						</li>
					{/each}
				</ol>
			{/if}
		{/if}
	</div>
</li>

<style>
	.zeile {
		border-top: 1px solid var(--line);
		padding: 8px 0;
	}
	.zeile:first-child {
		border-top: none;
	}
	.zeile.rot {
		border-left: 4px solid var(--bad);
		padding-left: 10px;
		background: color-mix(in srgb, var(--bad) 6%, transparent);
	}
	.kopf {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.auf {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px 10px;
		text-align: left;
		border: none;
		background: none;
		color: var(--text);
		font: inherit;
		padding: 4px 6px;
		border-radius: 8px;
		cursor: pointer;
	}
	.auf:hover {
		background: var(--accent-soft);
	}
	.titel {
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.projekt {
		white-space: nowrap;
	}
	.warnung {
		margin: 4px 6px 0;
		color: var(--bad-text);
		font-weight: 600;
	}
	.schritte {
		margin: 6px 6px 4px 30px;
	}
	.schritte li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px 8px;
		padding: 3px 0;
	}
	.zeit {
		min-width: 9em;
		font-variant-numeric: tabular-nums;
	}
	.notiz {
		font-style: italic;
	}
</style>
