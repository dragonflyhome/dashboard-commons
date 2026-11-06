<script lang="ts">
	/**
	 * Widget „Fristen“ (Etappe 3a-3, A4), gewählter Entwurf „Liste der
	 * Fristen“: die nächsten Fristen der offenen Einträge, die nächste zuerst
	 * und etwas größer, je mit „in N Tagen“ und einem Link auf den Eintrag;
	 * höchstens drei, dann „und N weitere“. Kennt das Widget keine Einträge
	 * (etwa ohne Anmeldung), steht nur die nächste Frist des Servers da.
	 * Ist die nächste Frist heute, ist der Rand gelb, ist sie vorbei, rot; das
	 * sagt die Frist-Stufe des Servers (`frist_stufe`), nicht die Zahl der Tage.
	 *
	 * Etappe B3: dazu die Fristen aus `daten/fristen.toml` mit Datum
	 * (`dateiFristen`, vom Server in `stand.fristen`) in derselben Liste; ihr
	 * Link zeigt auf `#frist-<kennung>` (die Fristenliste der Seite).
	 *
	 * Warum diese Form: Eine einzelne Frist (bisherige Karte „Nächste Frist“)
	 * verbirgt, dass zwei Tage später die nächste kommt; ein Kalenderblatt
	 * braucht Platz und sagt nicht, worum es geht. Die Liste nennt Datum,
	 * Abstand und Sache in einer Zeile und bricht am Handy sauber um.
	 */
	import type { AnstehendEintrag, FristStufe, FristZeile } from '$vertrag';
	import { fristPunkte, fristWorte, kurzDatum } from '#lib/anstehend.ts';
	import { klasse } from '#lib/bedeutung.ts';
	import { teilen } from '#lib/unterdashboard.ts';
	import { fristBedeutung } from './worte.ts';

	interface Props {
		/** Die nächste Frist (JJJJ-MM-TT), vom Server. */
		naechste: string;
		/** Tage bis dahin, vom Server (nur für „in N Tagen“). */
		tageBis: number | null;
		/** Wie nah die nächste Frist ist, vom Server; daraus die Farbe des Rands. */
		fristStufe: FristStufe | null;
		/** Offene Einträge; die mit Frist erscheinen in der Liste. */
		eintraege?: readonly AnstehendEintrag[];
		/** Etappe B3: offene Fristen aus fristen.toml mit Datum (vom Server). */
		dateiFristen?: readonly FristZeile[];
		/** Wie viele Fristen vorn stehen. */
		hoechstens?: number;
	}
	let {
		naechste,
		tageBis,
		fristStufe,
		eintraege = [],
		dateiFristen = [],
		hoechstens = 3
	}: Props = $props();

	// Geordnet wird in anstehend.ts (Etappe 3a+), auch mit den Fristen aus der Datei.
	const liste = $derived(teilen(fristPunkte(eintraege, dateiFristen), hoechstens));
	const ton = $derived(fristBedeutung(fristStufe));
</script>

{#snippet datum(frist: string, tage: number | null)}
	{@const worte = fristWorte(tage)}
	<b>{kurzDatum(frist)}</b>{#if worte}<span>, {worte}</span>{/if}
{/snippet}

<div
	class="karte widget card fristen {ton === null ? '' : klasse(ton)}"
	data-frist-stufe={fristStufe ?? 'keine'}
>
	<h2>Fristen</h2>
	{#if liste.vorn.length === 0}
		<p class="erste">{@render datum(naechste, tageBis)}</p>
	{:else}
		<ul class="plain widget-liste">
			{#each liste.vorn as e, i (e.schluessel)}
				<li class:erste={i === 0}>
					{@render datum(e.frist, e.tage_bis)}
					<span class="sache small"><a href="#{e.anker}">{e.titel}</a></span>
				</li>
			{/each}
		</ul>
		{#if liste.rest.length > 0}
			<details class="widget-mehr">
				<summary class="small">und {liste.rest.length} weitere</summary>
				<ul class="plain widget-liste">
					{#each liste.rest as e (e.schluessel)}
						<li>
							{@render datum(e.frist, e.tage_bis)}
							<span class="sache small"><a href="#{e.anker}">{e.titel}</a></span>
						</li>
					{/each}
				</ul>
			</details>
		{/if}
	{/if}
</div>

<style>
	.erste {
		font-size: var(--text-lg);
		margin-bottom: 0.2rem;
	}
	.sache {
		display: block;
	}
</style>
