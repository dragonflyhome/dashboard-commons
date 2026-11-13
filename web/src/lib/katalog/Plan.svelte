<script lang="ts">
	/**
	 * Plan (Etappe B8, Teil 3 der Projektseite): „Wo stehen wir im Ganzen?“
	 * Die Etappen aus `daten/plan.toml` in ihrer Reihenfolge, je Etappe zuerst
	 * das Stand-Wort (geplant, läuft seit …, fertig am …, vor dem Tracker
	 * abgeschlossen), dann „x von y Arbeiten fertig“; aufgeklappt das Ziel und
	 * die Arbeiten mit Stand-Wort, offene mit Link zu ihrer Zeile in der
	 * Aufgabenliste, fertige und archivierte nur hier. Stand, Zählung und
	 * Zeitpunkte rechnet der Server aus dem Tracker (`/plan`).
	 *
	 * Darüber ein Zeitstreifen nur aus Ist-Daten (`zeitstreifen`, Achse aus
	 * `#lib/zeit/achse.ts` wie im Balkenplan): fertige Etappen als Balken,
	 * laufende bis heute schraffiert, geplante ohne Balken mit dem Wort,
	 * echte Termine als Raute. Er ist nur Bild (`aria-hidden`); die Liste
	 * darunter ist seine Textfassung. Der kleine Balken je Etappe ist Schmuck,
	 * die Zahl steht einmal im Text daneben.
	 */
	import type { PlanArbeit, ProjektPlan } from '$vertrag';
	import { klasse } from '#lib/bedeutung.ts';
	import {
		ETAPPEN_BEDEUTUNG,
		anteil,
		etappenWort,
		gewachsenSatz,
		inListe,
		planArbeitWort,
		zaehlung,
		zeitstreifen
	} from '#lib/projektteile.ts';
	import { kurzDatum, tagText } from '#lib/zeit/tage.ts';
	import Marke from './Marke.svelte';

	interface Props {
		plan: ProjektPlan;
		/** Für Tests; sonst heute. */
		jetzt?: Date;
	}
	let { plan, jetzt = new Date() }: Props = $props();

	const streifen = $derived(zeitstreifen(plan, tagText(jetzt)));
</script>

{#snippet arbeiten(liste: readonly PlanArbeit[])}
	<ul class="arbeiten small">
		{#each liste as a (a.id)}
			<!-- Fertige und archivierte Arbeiten dieses Projekts haben ihr Ziel (`#arbeit-…`, etwa aus der Zeitleiste) hier; offene in der Aufgabenliste. -->
			<li id={inListe(a) || a.aus !== null ? undefined : `arbeit-${a.id}`}>
				{#if inListe(a)}
					<a href="#arbeit-{a.id}">{a.titel}</a>
				{:else}
					<span>{a.titel}</span>
				{/if}
				<span class="muted">· {planArbeitWort(a)}</span>
			</li>
		{/each}
	</ul>
{/snippet}

<div class="plan">
	{#each plan.hinweise as h, i (i)}
		<p class="muted small">{h}</p>
	{/each}
	{#if plan.etappen.length === 0}
		<p class="muted">
			Für dieses Projekt steht noch kein Plan in <code>daten/plan.toml</code>.
		</p>
	{:else}
		{#if streifen !== null}
			<div class="streifen" aria-hidden="true" data-streifen>
				{#each streifen.zeilen as z (z.kennung)}
					<span class="name">{z.titel}</span>
					<span class="bahn">
						{#if z.balken !== null}
							<i
								class="balken {klasse(ETAPPEN_BEDEUTUNG[z.stand])}"
								class:offen={z.balken.offen}
								style:left="{z.balken.links}%"
								style:width="{z.balken.breite}%"
							></i>
						{:else if z.stand === 'geplant'}
							<span class="geplant">geplant</span>
						{/if}
						{#if z.raute !== null}
							<i class="raute" style:left="{z.raute}%"></i>
						{/if}
						<i class="heute" style:left="{streifen.heute}%"></i>
					</span>
				{/each}
				<span class="achse">
					<span>{kurzDatum(streifen.von)}</span>
					<span>{streifen.bis === tagText(jetzt) ? 'heute ' : ''}{kurzDatum(streifen.bis)}</span>
				</span>
			</div>
		{/if}
		<ol class="plain etappen">
			{#each plan.etappen as e (e.kennung)}
				{@const zahl = zaehlung(e)}
				{@const gewachsen = gewachsenSatz(e)}
				<li>
					<details class="etappe" id="etappe-{e.kennung}">
						<summary>
							<span class="titel">{e.titel}</span>
							<Marke bedeutung={ETAPPEN_BEDEUTUNG[e.stand]} wort={etappenWort(e)} />
							{#if zahl !== null}
								<span class="zaehlung small">
									<span class="mini" aria-hidden="true"><i style:width="{anteil(e)}%"></i></span>
									{zahl}
								</span>
							{/if}
							{#if gewachsen !== null}<span class="gewachsen small muted">{gewachsen}</span>{/if}
						</summary>
						<p class="ziel small">{e.ziel}</p>
						{#if e.arbeiten.length > 0}
							{@render arbeiten(e.arbeiten)}
						{/if}
					</details>
				</li>
			{/each}
		</ol>
	{/if}
	{#if plan.ohne_etappe.length > 0}
		<h3 class="ohne">Ohne gültige Etappe</h3>
		<p class="muted small">
			Diese Arbeiten nennen eine Etappe, die in <code>daten/plan.toml</code> fehlt.
		</p>
		{@render arbeiten(plan.ohne_etappe)}
	{/if}
</div>

<style>
	.plan {
		margin: 0.2em 0 0.4em;
	}
	/* Name links, Bahn rechts; am Handy steht der Name über der Bahn. */
	.streifen {
		display: grid;
		grid-template-columns: minmax(6rem, 10rem) minmax(0, 1fr);
		gap: 0.25rem 0.6rem;
		align-items: center;
		margin: 0.2rem 0 1rem;
		font-size: var(--text-sm);
	}
	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.bahn {
		position: relative;
		height: 0.9rem;
		border-radius: 0.25rem;
		background: color-mix(in srgb, var(--line) 60%, transparent);
	}
	.balken,
	.raute,
	.heute {
		position: absolute;
		top: 0;
	}
	.balken {
		height: 100%;
		min-width: 0.2rem;
		border-radius: 0.25rem;
		background: var(--ton, var(--ok));
	}
	.balken.offen {
		background: repeating-linear-gradient(
			45deg,
			var(--ton, var(--plan)) 0 0.25rem,
			transparent 0.25rem 0.5rem
		);
		border: 1px solid var(--ton, var(--plan));
	}
	.raute {
		top: 0.1rem;
		width: 0.7rem;
		height: 0.7rem;
		margin-left: -0.35rem;
		background: var(--accent);
		transform: rotate(45deg);
	}
	.heute {
		height: 100%;
		border-left: 0.125rem solid var(--text);
	}
	.geplant {
		position: absolute;
		right: 0.3rem;
		top: -0.1rem;
		color: var(--muted);
		font-size: var(--text-xs);
	}
	.achse {
		grid-column: 2;
		display: flex;
		justify-content: space-between;
		color: var(--muted);
		font-size: var(--text-xs);
	}
	.etappen > li {
		border-top: 1px solid var(--line);
		padding: 0.4rem 0;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	summary {
		cursor: pointer;
	}
	.titel {
		font-weight: 600;
		margin-right: 0.5rem;
	}
	.zaehlung {
		margin-left: 0.5rem;
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		color: var(--muted);
	}
	.mini {
		display: inline-block;
		width: 5rem;
		height: 0.5rem;
		border-radius: 0.25rem;
		overflow: hidden;
		background: color-mix(in srgb, var(--line) 60%, transparent);
	}
	.mini i {
		display: block;
		height: 100%;
		background: var(--ok);
	}
	.gewachsen {
		display: block;
		margin-left: 1.1rem;
	}
	.ziel {
		margin: 0.3rem 0 0.2rem 1.1rem;
	}
	.arbeiten {
		margin: 0.2rem 0 0.2rem 1.1rem;
		padding-left: 1rem;
	}
	.ohne {
		margin: 1rem 0 0.2rem;
		font-size: var(--text-md);
	}
	@media (max-width: 37.5rem) {
		.streifen {
			grid-template-columns: minmax(0, 1fr);
		}
		.achse {
			grid-column: 1;
		}
	}
</style>
