<script lang="ts">
	/**
	 * Der Verlauf (Etappe 3a-1, Zeitachse Fassung 1): senkrechte Liste, neueste
	 * oben, gruppiert nach Tag (Heute, Gestern, Wochentag mit Datum), darüber
	 * Woche, Monat und Jahr. Heute und Gestern sind offen, alles andere lässt
	 * sich aufklappen; Routine eines Tages steht in einer Zeile. Farbe nach
	 * `art`, die Quelle klein rechts, Filter nach Quelle mit Anzahl.
	 *
	 * `modus="projekt"`: Kommendes über der Heute-Linie aufsteigend,
	 * Vergangenes darunter absteigend je Jahr, alles offen.
	 *
	 * Tastatur: Pfeile gehen von Eintrag zu Eintrag, Enter klappt auf, Pos1
	 * springt auf heute (die Gruppe Heute steht dafür auch leer da; ist gar
	 * nichts da, steht nur der Leer-Satz und Pos1 bleibt ohne Ziel).
	 * Bildschirmleser lesen Überschriften je Tag und darunter eine Liste. Die
	 * Ordnung rechnet `#lib/zeit/verlauf.ts`.
	 *
	 * Der Quellenfilter (`aus`) kann von außen kommen, damit der Dichtestreifen
	 * dieselben Quellen zählt. Ohne `jetzt` läuft Heute über Mitternacht
	 * weiter (`LaufendesJetzt`).
	 */
	import { tick, untrack } from 'svelte';
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import type { VerlaufEreignis, VerlaufQuelle } from '$vertrag';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import {
		QUELLE_KURZ,
		artWort,
		bedeutungAusArt,
		datumText,
		filtern,
		gruppeMitTag,
		gruppieren,
		istLang,
		projektVerlauf,
		quellenZaehlen,
		zeitText,
		type Gruppe,
		type Zeile,
		type Zeitraum
	} from '#lib/zeit/verlauf.ts';
	import { kurzDatum, tagText, tagTitel } from '#lib/zeit/tage.ts';
	import { LaufendesJetzt } from '#lib/zeit/jetzt.svelte.ts';

	interface Props {
		ereignisse: readonly VerlaufEreignis[];
		/** Hinweise des Servers zur Vollständigkeit; stehen ruhig unter der Liste. */
		hinweise?: readonly string[];
		modus?: 'pc' | 'projekt';
		/** Für Tests; sonst jetzt, über Mitternacht weiter. */
		jetzt?: Date;
		/** Ausgeblendete Quellen; von außen, wenn der Dichtestreifen mitzählen soll. */
		aus?: SvelteSet<VerlaufQuelle>;
		/** Zu diesem Tag springen (`nr` ändert sich bei jedem Sprung). */
		sprung?: { tag: string; nr: number } | null;
		/** Nur diese Tage zeigen (aus dem Dichtestreifen). */
		zeitraum?: Zeitraum | null;
		onzeitraumweg?: () => void;
		/** Ebene der Überschriften (h3 unter einer Karte mit h2). */
		ebene?: 2 | 3 | 4;
		/** Satz, wenn nichts da ist. */
		leer?: string;
	}
	let {
		ereignisse,
		hinweise = [],
		modus = 'pc',
		jetzt,
		aus = new SvelteSet<VerlaufQuelle>(),
		sprung = null,
		zeitraum = null,
		onzeitraumweg,
		ebene = 3,
		leer = 'Noch keine Einträge.'
	}: Props = $props();

	const h = $derived(`h${String(ebene)}`);
	const h2 = $derived(`h${String(Math.min(ebene + 1, 6))}`);
	const uhr = new LaufendesJetzt();
	const zeitpunkt = $derived(jetzt ?? uhr.wert);
	const heute = $derived(tagText(zeitpunkt));

	const umgeschaltet = new SvelteMap<string, boolean>();
	const quellen = $derived(quellenZaehlen(ereignisse));
	// Nur eine Quelle (Projektseiten: alles von Hand): die Angabe rechts wäre Rauschen.
	const zeigeQuelle = $derived(quellen.length > 1);
	const gefiltert = $derived(filtern(ereignisse, aus, zeitraum));
	const gruppen = $derived(modus === 'pc' ? gruppieren(gefiltert, zeitpunkt) : []);
	const projekt = $derived(modus === 'projekt' ? projektVerlauf(gefiltert, zeitpunkt) : null);

	function istOffen(g: Gruppe): boolean {
		return umgeschaltet.get(g.schluessel) ?? (zeitraum !== null || g.offen);
	}
	function umschalten(schluessel: string, jetztOffen: boolean): void {
		umgeschaltet.set(schluessel, !jetztOffen);
	}
	function quelleUmschalten(q: VerlaufQuelle): void {
		if (aus.has(q)) aus.delete(q);
		else aus.add(q);
	}
	function anzahl(n: number): string {
		return n === 1 ? '1 Eintrag' : `${String(n)} Einträge`;
	}

	let wurzel = $state<HTMLDivElement | null>(null);

	/** Ins Bild rollen, ohne Bewegung (ruhig; jsdom kennt scrollIntoView nicht). */
	function rollen(el: HTMLElement, block: ScrollLogicalPosition): void {
		if (typeof el.scrollIntoView === 'function') el.scrollIntoView({ block });
	}

	// Springen (Dichtestreifen): Gruppe öffnen, hinrollen, Fokus dorthin.
	let zielTag = $state<string | null>(null);
	// Ein Satz genügt (Auflage aus 3a-1): Ist gar nichts da, steht nur der
	// Leer-Satz; ist das Ziel die leere Gruppe Heute, sagt sie „Noch nichts.“
	const zielLeer = $derived(
		zielTag !== null &&
			modus === 'pc' &&
			ereignisse.length > 0 &&
			!gruppen.some((g) => g.tage.some((t) => t.tag === zielTag) || (g.heute && g.von === zielTag))
	);
	async function springen(tag: string): Promise<void> {
		zielTag = tag;
		const g = gruppeMitTag(gruppen, tag);
		if (g !== null) umgeschaltet.set(g.schluessel, true);
		await tick();
		const el =
			wurzel?.querySelector<HTMLElement>(`[data-tag="${tag}"]`) ??
			(g === null ? null : wurzel?.querySelector<HTMLElement>(`[data-gruppe="${g.schluessel}"]`));
		if (!el) return;
		rollen(el, 'start');
		el.focus({ preventScroll: true });
	}
	$effect(() => {
		const s = sprung;
		if (s === null) return;
		untrack(() => void springen(s.tag));
	});

	/** Ab so vielen Einträgen hat der Zeitstrahl eines Projekts den Knopf „Heute“. */
	const LANG = 6;
	/** Zur Heute-Linie (Knopf „Heute“, Etappe 3b): hinrollen, Fokus dorthin. */
	function zuHeute(): void {
		const el = wurzel?.querySelector<HTMLElement>('.heute-linie');
		if (!el) return;
		rollen(el, 'center');
		el.focus({ preventScroll: true });
	}

	// Tastatur: Pfeile von Ziel zu Ziel, Pos1 auf heute, Enter klappt Texte auf.
	function ziele(): HTMLElement[] {
		if (!wurzel) return [];
		return [...wurzel.querySelectorAll<HTMLElement>('[data-verlauf-ziel]')].filter(
			(e) => e.getClientRects().length > 0
		);
	}
	function onkeydown(e: KeyboardEvent): void {
		if (e.altKey || e.ctrlKey || e.metaKey) return;
		const aktiv = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>(
			'[data-verlauf-ziel]'
		);
		const liste = ziele();
		const i = aktiv ? liste.indexOf(aktiv) : -1;
		let neu: HTMLElement | undefined;
		switch (e.key) {
			case 'ArrowDown':
				neu = liste[Math.min(i + 1, liste.length - 1)];
				break;
			case 'ArrowUp':
				neu = liste[Math.max(i - 1, 0)];
				break;
			case 'Home':
				neu =
					wurzel?.querySelector<HTMLElement>('[data-heute][data-verlauf-ziel]') ??
					wurzel?.querySelector<HTMLElement>('[data-heute] [data-verlauf-ziel]') ??
					liste[0];
				break;
			case 'End':
				neu = liste[liste.length - 1];
				break;
			case 'Enter':
			case ' ': {
				// Nur der Eintrag selbst klappt auf; Enter auf einem Link oder Knopf darin
				// bleibt bei diesem (Link folgen, Knopf drücken).
				if (aktiv === null || aktiv === undefined || document.activeElement !== aktiv) return;
				const mehr = aktiv.dataset.mehr;
				if (aktiv.tagName !== 'DIV' || mehr === undefined) return;
				umschalten(mehr, umgeschaltet.get(mehr) ?? false);
				e.preventDefault();
				return;
			}
			default:
				return;
		}
		if (!neu) return;
		e.preventDefault();
		neu.focus({ preventScroll: true });
		rollen(neu, 'nearest');
	}
</script>

{#snippet eintragZeile(z: Zeile, mitDatum: boolean)}
	{#if z.art === 'routine'}
		{@const offen = umgeschaltet.get(z.schluessel) ?? false}
		<li class="zeile routine">
			<button
				type="button"
				class="routine-knopf"
				data-verlauf-ziel
				tabindex="-1"
				aria-expanded={offen}
				onclick={() => {
					umschalten(z.schluessel, offen);
				}}
			>
				<StatusDot bedeutung="neutral" decorative />
				<span class="wann"
					>{z.eintraege.length > 0 && z.eintraege[0] ? zeitText(z.eintraege[0]) : ''}</span
				>
				<span class="was">{z.titel} <span aria-hidden="true">{offen ? '▾' : '▸'}</span></span>
				<span class="quelle">{QUELLE_KURZ[z.quelle]}</span>
			</button>
			{#if offen}
				<ul class="unter">
					{#each z.eintraege as x (x.e.kennung)}
						{@render eintragZeile(
							{ art: 'eintrag', schluessel: x.e.kennung, eintrag: x },
							mitDatum
						)}
					{/each}
				</ul>
			{/if}
		</li>
	{:else}
		{@const x = z.eintrag}
		{@const lang = istLang(x.e.text)}
		{@const offen = umgeschaltet.get(z.schluessel) ?? false}
		<li class="zeile" data-art={x.e.art}>
			<div
				class="eintrag"
				data-verlauf-ziel
				data-kennung={x.e.kennung}
				data-mehr={lang ? z.schluessel : undefined}
				tabindex="-1"
			>
				<StatusDot bedeutung={bedeutungAusArt(x.e.art)} decorative />
				<span class="wann">{mitDatum ? datumText(x) : zeitText(x)}</span>
				<span class="was">
					<span class="unsichtbar">{artWort(x.e.art)}: </span>
					{#if x.e.verweis}
						<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- Adresse im Dashboard vom Server, kein Routenname -->
						<a class="titel" href={x.e.verweis}>{x.e.titel}</a>
					{:else}
						<span class="titel">{x.e.titel}</span>
					{/if}
					{#if x.e.text}
						<span class="text" class:zu={lang && !offen}>{x.e.text}</span>
						{#if lang}
							<button
								type="button"
								class="mehr"
								tabindex="-1"
								aria-expanded={offen}
								onclick={() => {
									umschalten(z.schluessel, offen);
								}}>{offen ? 'weniger' : 'mehr'}</button
							>
						{/if}
					{/if}
				</span>
				{#if zeigeQuelle}<span class="quelle">{QUELLE_KURZ[x.e.quelle]}</span>{/if}
			</div>
		</li>
	{/if}
{/snippet}

<!-- svelte-ignore a11y_no_static_element_interactions (die Hülle fängt nur die Pfeiltasten der fokussierten Einträge darin ab; selbst bekommt sie keinen Fokus) -->
<div
	class="verlauf"
	class:projekt={modus === 'projekt'}
	data-zeit="verlauf"
	bind:this={wurzel}
	{onkeydown}
>
	{#if quellen.length > 1}
		<div class="filter" role="group" aria-label="Nach Quelle filtern">
			{#each quellen as q (q.quelle)}
				<button
					type="button"
					class="pill"
					aria-pressed={!aus.has(q.quelle)}
					onclick={() => {
						quelleUmschalten(q.quelle);
					}}>{q.name} <span class="zahl">{q.anzahl}</span></button
				>
			{/each}
		</div>
	{/if}
	{#if zeitraum !== null}
		<p class="zeitraum" data-zeitraum>
			Nur {kurzDatum(zeitraum.von)} bis {kurzDatum(zeitraum.bis)}
			<button type="button" class="pill" onclick={() => onzeitraumweg?.()}>alle Tage zeigen</button>
		</p>
	{/if}

	{#if modus === 'pc'}
		{#if zielLeer && zielTag !== null}
			<p class="leer-tag" data-tag={zielTag} tabindex="-1">
				{tagTitel(zielTag, heute)}: An diesem Tag steht nichts im Verlauf.
			</p>
		{/if}
		{#if ereignisse.length === 0}
			<!-- Gar nichts da (etwa die Startseite nach ruhigen Tagen): der Satz statt leerer Gruppen. -->
			<p class="nichts leer">{leer}</p>
		{/if}
		{#each ereignisse.length === 0 ? [] : gruppen as g (g.schluessel)}
			{@const offen = istOffen(g)}
			<section
				class="gruppe"
				data-gruppe={g.schluessel}
				data-heute={g.heute ? '' : undefined}
				data-tag={g.stufe === 'tag' ? g.von : undefined}
				tabindex="-1"
				aria-labelledby="kopf-{g.schluessel}"
			>
				<svelte:element this={h} class="kopf" id="kopf-{g.schluessel}">
					<button
						type="button"
						data-verlauf-ziel
						aria-expanded={offen}
						onclick={() => {
							umschalten(g.schluessel, offen);
						}}
					>
						<span class="pfeil" aria-hidden="true">{offen ? '▾' : '▸'}</span>
						<span>{g.titel}</span>
						<span class="anzahl">{anzahl(g.anzahl)}</span>
					</button>
				</svelte:element>
				{#if offen}
					{#if g.anzahl === 0}
						<p class="nichts">Noch nichts.</p>
					{/if}
					{#each g.tage as t (t.tag)}
						{#if g.stufe !== 'tag'}
							<svelte:element this={h2} class="tag-kopf" data-tag={t.tag} tabindex="-1"
								>{t.titel} <span class="anzahl">{anzahl(t.anzahl)}</span></svelte:element
							>
						{/if}
						<ul class="liste">
							{#each t.zeilen as z (z.schluessel)}
								{@render eintragZeile(z, false)}
							{/each}
						</ul>
					{/each}
					{#if g.ohneTag.length > 0}
						{#if g.tage.length > 0}
							<svelte:element this={h2} class="tag-kopf">Ohne genauen Tag</svelte:element>
						{/if}
						<ul class="liste">
							{#each g.ohneTag as z (z.schluessel)}
								{@render eintragZeile(z, false)}
							{/each}
						</ul>
					{/if}
				{/if}
			</section>
		{/each}
	{:else if projekt !== null}
		{#if projekt.kommend.length === 0 && projekt.vergangen.length === 0}
			<p class="nichts">{leer}</p>
		{/if}
		{#if projekt.kommend.length + projekt.vergangen.reduce((n, a) => n + a.eintraege.length, 0) >= LANG}
			<!-- Etappe 3b (Fund des Prüfers „auf 4 Zeitstrahlen fehlt der Knopf“): bei einer
			     langen Liste springt „Heute“ zur Heute-Linie, wie Pos1. -->
			<p class="leiste">
				<button type="button" class="pill" onclick={zuHeute}>Heute</button>
			</p>
		{/if}
		{#if projekt.kommend.length > 0}
			<svelte:element this={h} class="abschnitt">Kommt noch</svelte:element>
			<ul class="liste">
				{#each projekt.kommend as x (x.e.kennung)}
					{@render eintragZeile({ art: 'eintrag', schluessel: x.e.kennung, eintrag: x }, true)}
				{/each}
			</ul>
		{/if}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex (Einstieg für die Tastatur: Tab landet auf heute, von dort gehen die Pfeile weiter) -->
		<p class="heute-linie" data-heute data-verlauf-ziel tabindex="0">
			<span>Heute, {kurzDatum(heute, true)}</span>
		</p>
		{#each projekt.vergangen as a (a.schluessel)}
			<svelte:element this={h} class="abschnitt" data-jahr={a.titel}>{a.titel}</svelte:element>
			<ul class="liste">
				{#each a.eintraege as x (x.e.kennung)}
					{@render eintragZeile({ art: 'eintrag', schluessel: x.e.kennung, eintrag: x }, true)}
				{/each}
			</ul>
		{/each}
	{/if}

	{#if hinweise.length > 0}
		<details class="hinweise">
			<summary>
				{hinweise.length === 1 ? '1 Hinweis' : `${String(hinweise.length)} Hinweise`} zur Vollständigkeit
			</summary>
			<ul>
				{#each hinweise as t, i (i)}
					<li>{t}</li>
				{/each}
			</ul>
		</details>
	{/if}
</div>

<style>
	.verlauf {
		container-type: inline-size;
		margin: 0.5rem 0 1rem;
	}
	.filter {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
		margin: 0 0 0.75rem;
	}
	.pill {
		border: 0.0625rem solid var(--line);
		cursor: pointer;
		font: inherit;
		font-size: var(--text-xs);
	}
	.pill[aria-pressed='false'] {
		opacity: 0.5;
		text-decoration: line-through;
	}
	.zahl {
		color: var(--muted);
		font-variant-numeric: tabular-nums;
	}
	.leiste {
		display: flex;
		justify-content: flex-end;
		margin: 0 0 0.25rem;
	}
	.leiste .pill {
		min-width: 2.75rem;
		min-height: 2.75rem;
	}
	.zeitraum {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		font-size: var(--text-md);
	}
	.gruppe {
		scroll-margin-top: 5rem;
		border-top: 0.0625rem solid var(--line);
	}
	.gruppe:focus {
		outline: none;
	}
	.kopf {
		margin: 0;
		font-size: var(--text-md);
	}
	.kopf button {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		width: 100%;
		padding: 0.5rem 0.25rem;
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
		font-weight: 650;
		text-align: left;
		cursor: pointer;
		border-radius: 0.375rem;
	}
	.kopf button:hover {
		background: var(--accent-soft);
	}
	.pfeil {
		width: 0.75rem;
		color: var(--muted);
	}
	.anzahl {
		margin-left: auto;
		font-size: var(--text-xs);
		font-weight: 400;
		color: var(--muted);
		white-space: nowrap;
	}
	.tag-kopf {
		display: flex;
		gap: 0.5rem;
		margin: 0.5rem 0 0.125rem 1.25rem;
		font-size: var(--text-sm);
		color: var(--muted);
		scroll-margin-top: 5rem;
	}
	.abschnitt {
		margin: 0.75rem 0 0.25rem;
		font-size: var(--text-md);
		color: var(--muted);
	}
	.liste,
	.unter {
		list-style: none;
		margin: 0 0 0.5rem;
		padding: 0;
	}
	.unter {
		margin: 0 0 0.25rem 1.25rem;
		border-left: 0.125rem solid var(--line);
	}
	.eintrag,
	.routine-knopf {
		display: grid;
		grid-template-columns: 0.75rem 5.5rem minmax(0, 1fr) auto;
		align-items: baseline;
		gap: 0.25rem 0.5rem;
		width: 100%;
		padding: 0.3rem 0.25rem;
		border-radius: 0.375rem;
	}
	.projekt .eintrag {
		grid-template-columns: 0.75rem 9.5rem minmax(0, 1fr) auto;
	}
	.routine-knopf {
		border: 0;
		background: none;
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}
	.routine-knopf:hover,
	.eintrag:focus-visible {
		background: var(--accent-soft);
	}
	.wann {
		color: var(--muted);
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.was {
		min-width: 0;
	}
	.titel {
		font-weight: 600;
	}
	.routine .was {
		color: var(--muted);
	}
	.text {
		display: block;
		color: var(--muted);
		font-size: var(--text-md);
		white-space: pre-line;
	}
	.text.zu {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.mehr {
		padding: 0;
		border: 0;
		background: none;
		color: var(--accent);
		font: inherit;
		font-size: var(--text-sm);
		cursor: pointer;
	}
	.quelle {
		font-size: var(--text-2xs);
		color: var(--muted);
		white-space: nowrap;
	}
	.zeile[data-art='abgebrochen'] .titel {
		text-decoration: line-through;
		font-weight: 400;
	}
	.heute-linie {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin: 0.5rem 0;
		color: var(--accent);
		font-size: var(--text-sm);
		font-weight: 650;
		border-radius: 0.375rem;
	}
	.heute-linie::after {
		content: '';
		flex: 1;
		height: 0.125rem;
		background: var(--accent);
		opacity: 0.6;
	}
	.nichts,
	.leer-tag {
		margin: 0 0 0.5rem 1.25rem;
		color: var(--muted);
		font-size: var(--text-md);
		scroll-margin-top: 5rem;
	}
	.nichts.leer {
		margin-left: 0;
	}
	.hinweise {
		margin-top: 0.75rem;
		font-size: var(--text-xs);
		color: var(--muted);
	}
	.hinweise summary {
		cursor: pointer;
	}
	.hinweise ul {
		margin: 0.25rem 0 0;
		padding-left: 1.25rem;
	}
	.unsichtbar {
		position: absolute;
		width: 0.0625rem;
		height: 0.0625rem;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
	/* Handy: eine Spalte; Zeit und Quelle über dem Titel. */
	@container (max-width: 30rem) {
		/* Etappe 3b: am Handy Ziele mindestens 44 px. */
		.pill,
		.kopf button {
			min-height: 2.75rem;
		}
		.eintrag,
		.routine-knopf,
		.projekt .eintrag {
			grid-template-columns: 0.75rem minmax(0, 1fr) auto;
		}
		.was {
			grid-column: 2 / 4;
		}
		.quelle {
			grid-column: 3;
			grid-row: 1;
		}
	}
</style>
