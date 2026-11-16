<script lang="ts">
	/**
	 * Seite /einstellungen (Etappe S2): die Farbwahl als Karten mit Vorschau,
	 * statt des Schalters unten auf jeder Seite, und was über dieses Dashboard
	 * zu wissen ist. Die Wahl gilt sofort und nur in diesem Browser
	 * (#lib/thema.ts); nach jeder Wahl nennt eine leise Zeile, was jetzt gilt,
	 * und „Rückgängig“ stellt die vorige Wahl wieder her.
	 *
	 * Die Karten zeichnet der Baustein `Wahl` in seiner Karten-Form: Tastatur
	 * und Rollen kommen von dort. Jede Karte zeigt ihr Thema selbst, über
	 * `data-thema-vorschau`. „Wie mein PC“ hat kein eigenes Thema: Die Karte
	 * zeigt das, was der Rechner gerade vorgibt, nennt es und folgt einem
	 * Wechsel.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Abschnitt, Absatz, Wahl, type KartenIds, type Option } from '$katalog';
	import { THEMEN, istThema, themaLesen, themaSetzen, type Thema } from '#lib/thema.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';

	/** Erklärung auf der Karte „Wie mein PC“. */
	const WIE_MEIN_PC = 'Folgt deinem PC: hell oder dunkel, wie er gerade eingestellt ist.';

	let thema = $state<Thema>('system');
	/** Was zuletzt angewendet wurde; daraus wird beim Wechsel die vorige Wahl. */
	let stand: Thema = 'system';
	let vorige = $state<Thema | null>(null);
	let meldung = $state('');
	let systemDunkel = $state(false);

	/** Das Thema, das der Rechner ohne eigene Wahl gerade vorgibt. */
	const geltend = $derived<Exclude<Thema, 'system'>>(systemDunkel ? 'dunkel' : 'hell');

	function name(t: Thema): string {
		return THEMEN.find((x) => x.wert === t)?.text ?? t;
	}

	onMount(() => {
		thema = themaLesen();
		stand = thema;
		const abfrage = window.matchMedia('(prefers-color-scheme: dark)');
		systemDunkel = abfrage.matches;
		const hoeren = (e: MediaQueryListEvent): void => {
			systemDunkel = e.matches;
		};
		abfrage.addEventListener('change', hoeren);
		return () => {
			abfrage.removeEventListener('change', hoeren);
		};
	});

	function anwenden(neu: Thema): void {
		if (neu === stand) return;
		vorige = stand;
		stand = neu;
		thema = neu;
		themaSetzen(neu);
		meldung = `Farben: ${name(neu)}.`;
	}

	function zurueck(): void {
		if (vorige !== null) anwenden(vorige);
	}
</script>

<svelte:head>
	<title>{seitenTitel('Einstellungen')}</title>
</svelte:head>

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Einstellungen</p>
<h1>Einstellungen</h1>

<Abschnitt titel="Aussehen">
	<Absatz
		>Wähle, wie dein Dashboard aussieht. Die Wahl gilt sofort und nur in diesem Browser.</Absatz
	>
	<Wahl
		bind:wert={thema}
		beschriftung="Farben"
		optionen={THEMEN}
		onwechsel={(w) => {
			if (istThema(w)) anwenden(w);
		}}
	>
		{#snippet karte(o: Option, ids: KartenIds)}
			{@const zeigt = o.wert === 'system' ? geltend : o.wert}
			<span class="vorschau" data-thema-vorschau={zeigt}>
				<span id={ids.name} class="v-name">{o.text}</span>
				<span class="v-muster" aria-hidden="true">
					<span class="v-neben">So sieht Nebentext aus.</span>
					<span class="v-akzent">Akzent</span>
					<span class="v-punkte">
						<span class="v-punkt ok"></span>
						<span class="v-punkt wartet"></span>
						<span class="v-punkt problem"></span>
					</span>
				</span>
			</span>
			<span id={ids.beschreibung} class="v-text">
				{#if o.wert === 'system'}
					<span>{WIE_MEIN_PC}</span>
					<span class="v-gerade">Gerade: {name(geltend)}</span>
				{/if}
			</span>
		{/snippet}
	</Wahl>
	<p class="wahl-zeile small">
		<span role="status">{meldung}</span>
		{#if vorige !== null}
			<button type="button" class="zurueck" onclick={zurueck}>Rückgängig</button>
		{/if}
	</p>
	<Absatz>Schriftgröße und weniger Bewegung kommen von deinem System.</Absatz>
</Abschnitt>

<Abschnitt titel="Dieses Dashboard">
	<Absatz>Dein Dashboard läuft nur hier auf diesem PC.</Absatz>
	<Absatz>Deine Wahl hier bleibt in diesem Browser und geht nirgendwohin.</Absatz>
</Abschnitt>

<style>
	/* Die Vorschau zeichnet mit den Token ihres eigenen Themas (data-thema-vorschau). */
	.vorschau {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		padding: 0.6rem 0.7rem;
		border: 1px solid var(--line);
		border-radius: calc(var(--radius) - 4px);
		background: var(--card);
		color: var(--text);
	}
	.v-name {
		font-weight: 600;
		/* Platz für den Haken der gewählten Karte. */
		padding-right: 1.5rem;
	}
	.v-muster {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.v-neben {
		color: var(--muted);
		font-size: var(--text-sm);
	}
	.v-akzent {
		color: var(--accent-text);
		font-size: var(--text-sm);
	}
	.v-punkte {
		display: flex;
		gap: 0.4rem;
	}
	.v-punkt {
		width: 0.75rem;
		height: 0.75rem;
		border-radius: 999px;
	}
	.v-punkt.ok {
		background: var(--ok);
	}
	.v-punkt.wartet {
		background: var(--warn);
	}
	.v-punkt.problem {
		background: var(--bad);
	}
	.v-text {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		color: var(--muted);
		font-size: var(--text-sm);
	}
	.v-gerade {
		color: var(--text);
	}
	.wahl-zeile {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem 0.8rem;
		min-height: 2.75rem;
		margin: 0.5rem 0;
		color: var(--muted);
	}
	.zurueck {
		min-height: 2.75rem;
		padding: 0 0.9rem;
		border: 1px solid var(--line);
		border-radius: 999px;
		background: var(--card);
		color: var(--text);
		font: inherit;
		font-size: var(--text-sm);
		cursor: pointer;
	}
	.zurueck:hover {
		background: var(--accent-soft);
	}
</style>
