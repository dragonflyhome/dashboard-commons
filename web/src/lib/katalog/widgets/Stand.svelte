<script lang="ts">
	/**
	 * Widget „Stand“ (Etappe 3a-3, A4), gewählter Entwurf „Kachel mit Ampel“:
	 * oben ein Rand in der Farbe der Ampel, darunter Punkt und Satz des
	 * Servers, dann die Zahlen in einer Zeile (wartet auf dich, läuft, als
	 * Nächstes; nur, was nicht null ist), dann Status, Schwerpunkt und
	 * „zuletzt geändert“. Die Ampel ist eine Regel des Servers; Satz und
	 * Zahlen kommen aus `standAnsicht` (anstehend.ts), Farbe und Wörter
	 * wählt dieses Widget.
	 *
	 * Warum diese Form: Die Ampel allein sagt nicht, wie viel ansteht; die
	 * kompakte Karte (bisher) brauchte dafür die anderen Karten. Die Zahlen
	 * stehen hier ruhig als Wörter mit Zahl, die Farbe nie allein (Punkt mit
	 * Namen „Ampel gelb“, Satz daneben).
	 *
	 * Etappe B8 (`teil`): der Teil „Stand“ der Projektseite, ohne eigene Karte
	 * (die Überschrift trägt der Abschnitt). Ein Satz, vier Zahlen (laufen,
	 * warten auf dich, als Nächstes, fertig in 7 Tagen), jede ein Sprung zu
	 * ihrer Gruppe; daneben die Ampel als Bedeutung in Worten mit Grund-Satz
	 * und Link („Wartet: „…“ wartet seit 9 Tagen auf dich.“). Darunter, was der
	 * Aufrufer klein dazugibt (Bau, Notizen).
	 */
	import type { Snippet } from 'svelte';
	import type { GesamtAnsicht, KurzAnsicht } from '#lib/anstehend.ts';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { klasse } from '#lib/bedeutung.ts';
	import { AMPEL_WORT, ampelZiel, standSatz, standZahlen } from '#lib/projektteile.ts';
	import { AMPEL } from './worte.ts';

	interface Props {
		gesamt: GesamtAnsicht;
		schwerpunkt: string;
		kurz?: KurzAnsicht | null;
		/** Der Datenspeicher fehlt gerade (ein Satz dazu). */
		speicherFehlt?: boolean;
		/** Etappe B8: als Teil der Projektseite (Projekt und fertige Arbeiten der letzten 7 Tage). */
		teil?: { slug: string; fertig7: number } | null;
		/** Nur mit `teil`: klein darunter (Bau, Notizen). */
		children?: Snippet;
	}
	let {
		gesamt,
		schwerpunkt,
		kurz = null,
		speicherFehlt = false,
		teil = null,
		children
	}: Props = $props();

	// Archiviert (`ruhig`): keine Ampel und keine Farbe, nur der ruhige Satz.
	const ruhig = $derived(gesamt.ruhig === true);
	const a = $derived(ruhig ? { bedeutung: 'neutral' as const, wort: '' } : AMPEL[gesamt.ampel]);
	const zahlen = $derived(
		kurz === null
			? []
			: [
					{ zahl: kurz.wartet_auf_dich, wort: 'wartet auf dich' },
					{ zahl: kurz.laeuft, wort: 'läuft' },
					{ zahl: kurz.als_naechstes, wort: 'als Nächstes' }
				].filter((z) => z.zahl > 0)
	);
	const ziel = $derived(teil === null ? null : ampelZiel(gesamt.grund_art, teil.slug));
</script>

{#snippet speicher()}
	{#if speicherFehlt}
		<p class="muted small">
			Aufgaben und Arbeiten fehlen gerade: Der Datenspeicher ist nicht offen. Notizen und Fragen
			stehen trotzdem da.
		</p>
	{/if}
{/snippet}

{#if teil !== null}
	<div class="teil">
		{#if kurz !== null}
			<p class="satz">{standSatz(kurz, teil.fertig7)}</p>
			<ul class="plain vier">
				{#each standZahlen(teil.slug, kurz, teil.fertig7) as z (z.ziel)}
					<li><a class="zahl" href="#{z.ziel}"><b>{z.zahl}</b> <span>{z.wort}</span></a></li>
				{/each}
			</ul>
		{/if}
		<p class="ampel {ruhig ? '' : klasse(a.bedeutung)}">
			{#if !ruhig}<StatusDot bedeutung={a.bedeutung} decorative /><b>{AMPEL_WORT[gesamt.ampel]}:</b
				>{/if}
			{#if ziel !== null && !ruhig}<a href="#{ziel}">{gesamt.grund}</a>{:else}{gesamt.grund}{/if}
		</p>
		{@render speicher()}
		{@render children?.()}
	</div>
{:else}
	<div class="karte widget card stand {ruhig ? 'ruhig' : klasse(a.bedeutung)}">
		<h2>Stand</h2>
		<p class="grund">
			{#if !ruhig}<StatusDot bedeutung={a.bedeutung} label="Ampel {a.wort}" />{/if}
			{gesamt.grund}
		</p>
		{#if zahlen.length > 0}
			<ul class="plain zahlen">
				{#each zahlen as z (z.wort)}
					<li><b>{z.zahl}</b> {z.wort}</li>
				{/each}
			</ul>
		{/if}
		<p class="muted small">
			{gesamt.status} · {schwerpunkt}
			{#if gesamt.zuletzt}<br />zuletzt geändert {gesamt.zuletzt}{/if}
		</p>
		{@render speicher()}
	</div>
{/if}

<style>
	.grund {
		font-weight: 600;
		margin-bottom: 0.3rem;
	}
	.zahlen {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 0.5rem;
		margin-bottom: 0.4rem;
		font-size: var(--text-sm);
	}
	.zahlen li {
		padding: 0.05rem 0.5rem;
		border-radius: 999px;
		background: var(--ton-soft, var(--plan-soft));
	}
	.satz {
		margin: 0 0 0.5rem;
	}
	/* Vier Zahlen nebeneinander, am Handy zwei je Zeile. */
	.vier {
		display: grid;
		grid-template-columns: repeat(4, minmax(0, 1fr));
		gap: 0.5rem;
		margin: 0 0 0.6rem;
	}
	.zahl {
		display: block;
		height: 100%;
		padding: 0.4rem 0.6rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		color: inherit;
		text-decoration: none;
	}
	.zahl b {
		display: block;
		font-size: var(--text-xl);
		line-height: 1.1;
	}
	.zahl span {
		color: var(--muted);
		font-size: var(--text-sm);
	}
	.zahl:hover span {
		text-decoration: underline;
	}
	.ampel {
		margin: 0 0 0.4rem;
		padding: 0.3rem 0.6rem;
		border-radius: var(--radius);
		background: var(--ton-soft, transparent);
		overflow-wrap: anywhere;
	}
	.ampel b {
		margin: 0 0.3rem 0 0.2rem;
	}
	@media (max-width: 37.5rem) {
		.vier {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
