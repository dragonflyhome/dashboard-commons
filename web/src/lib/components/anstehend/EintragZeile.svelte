<script lang="ts">
	/**
	 * Ein Eintrag in „Anstehend“: Art (Zeichen und Wort), Titel, Projekt als
	 * Link auf das Ziel (die Projektseite mit Anker), Schwerpunkt, Frist,
	 * Vermerke; darunter Umstufen, bei Aufgaben der Haken und bei Fragen die
	 * Antwort (Etappe A2: Knöpfe, Textfeld oder „Bestätigen“). Der Klick-Link
	 * einer Aufgabe steht hier nie (die Liste kennt ihn nicht, nur `hat_link`);
	 * „Öffnen“ gibt es auf der Projektseite.
	 *
	 * Meldungen (Etappe A3) haben eine feste Stufe: statt Umstufen steht dort
	 * „gelesen“; Systemmeldungen führen zur PC-Seite.
	 */
	import { resolve } from '$app/paths';
	import FrageAntwort from './FrageAntwort.svelte';
	import EtwasAnderes from '../EtwasAnderes.svelte';
	import { aufgabeEntwurf } from '#lib/etwas-anderes.ts';
	import { frageSchluessel } from '#lib/antworten.ts';
	import { zielHref } from '#lib/zieladresse.ts';
	import {
		ART_WORT,
		ART_ZEICHEN,
		MELDUNG_WORT,
		STUFEN,
		STUFE_WORT,
		ZUSTAND_BEIM_CHAT,
		anker,
		fristDringend,
		fristWorte,
		kurzDatum,
		vermerke,
		zustandWort,
		meldungsOrt
	} from '#lib/anstehend.ts';
	import type { AnstehendEintrag, Stufe } from '$vertrag';
	import { haengtSatz } from '#lib/fristen.ts';

	interface Props {
		eintrag: AnstehendEintrag;
		/** Schwerpunkt nennen (in der Ansicht „nach Schwerpunkt“ steht er schon darüber). */
		mitSchwerpunkt?: boolean;
		/** Gerade läuft etwas (Knöpfe gesperrt). */
		gesperrt?: boolean;
		onumstufen: (e: AnstehendEintrag, stufe: Stufe | null) => void;
		onhaken: (id: string, antwort: string | null) => void;
		/** „Etwas anderes …“ bei einer Aufgabe: eigene Worte; gibt einen Satz zurück, wenn es nicht ging. */
		oneigene: (id: string, eigene: string) => Promise<string | null>;
		/** Nach dem Beantworten einer Frage (die Liste lädt neu). */
		onbeantwortet?: () => void | Promise<void>;
		/** Meldung als gelesen ablegen (Etappe A3). */
		ongelesen?: (e: AnstehendEintrag, knopf: HTMLButtonElement) => void;
		/** Wo die Zeile steht (für den Fokus nach einer Antwort, Auflage aus A2). */
		abschnitt?: string;
	}
	let {
		eintrag: e,
		mitSchwerpunkt = true,
		gesperrt = false,
		onumstufen,
		onhaken,
		oneigene,
		onbeantwortet,
		ongelesen,
		abschnitt = 'anstehend'
	}: Props = $props();

	const kennung = $props.id();
	const gewaehlt = $derived(e.stufe.inhaber ?? e.stufe.vorschlag);
	const frist = $derived(fristWorte(e.tage_bis));
	// Etappe 3a-4: hervorgehoben nach der Frist-Stufe des Servers, derselben
	// Quelle, aus der er die Stufe hebt (und damit Rand und Reihenfolge).
	const achtung = $derived(fristDringend(e.frist_stufe));
	const hash = $derived.by(() => {
		// Nur den Anker aus `ziel` übernehmen, die Adresse baut resolve().
		const i = e.ziel.indexOf('#');
		return i >= 0 ? e.ziel.slice(i + 1) : anker(e);
	});
	// Entscheidung projektgruppe: mit dem Gruppentitel davor (`meldungsOrt`).
	const projektName = $derived(meldungsOrt(e));
	const meldungHref = $derived(e.art === 'meldung' && e.ziel_da ? zielHref(e.ziel) : null);
	const zustand = $derived(zustandWort(e));
</script>

<li
	class="eintrag"
	class:jetzt={e.stufe.wirksam === 'jetzt'}
	data-frist-stufe={e.frist_stufe ?? 'keine'}
>
	<div class="kopf">
		<span class="art art-{e.art}"
			><span aria-hidden="true">{ART_ZEICHEN[e.art]}</span>
			{e.meldung ? `${ART_WORT[e.art]}: ${MELDUNG_WORT[e.meldung.art]}` : ART_WORT[e.art]}</span
		>
		<p class="titel" id="{kennung}-titel">{e.titel}</p>
	</div>
	<p class="meta small">
		{#if meldungHref !== null}
			<!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- zielHref baut mit resolve() -->
			<a href={meldungHref}>{projektName}</a>
		{:else if e.ziel_da && e.art !== 'meldung'}
			<a href="{resolve('/projects/[slug]', { slug: e.projekt })}#{hash}">{projektName}</a>
		{:else}
			<span>{projektName} <span class="muted">(nicht mehr da)</span></span>
		{/if}
		{#if mitSchwerpunkt}
			<span class="muted">· {e.schwerpunkt}</span>
		{/if}
		{#if zustand}
			<span class="muted">· {zustand}</span>
		{/if}
		{#if e.frist}
			<span class="frist" class:achtung
				>· bis {kurzDatum(e.frist)}{#if frist}, {frist}{/if}</span
			>
		{:else if haengtSatz(e)}
			<!-- Etappe B3: bewegliche Frist, deren Ereignis noch kein Datum hat. -->
			<span class="frist">· {haengtSatz(e)}</span>
		{/if}
	</p>
	{#if vermerke(e).length > 0}
		<ul class="plain vermerke" aria-label="Vermerke">
			{#each vermerke(e) as v (v)}
				<li class="pill">{v}</li>
			{/each}
		</ul>
	{/if}
	{#if e.eigene}
		<!-- Aufgabe beim Chat: deine eigenen Worte („Etwas anderes …“). -->
		<p class="small text">Deine Antwort: {e.eigene}</p>
	{/if}
	{#if e.meldung?.text}
		<p class="small text">{e.meldung.text}</p>
	{/if}
	{#if e.art === 'frage'}
		{#key frageSchluessel(e)}
			<FrageAntwort eintrag={e} {onbeantwortet} {abschnitt} />
		{/key}
	{/if}
	{#if e.art === 'meldung'}
		<div class="handlung">
			<span class="muted small">feste Stufe für Meldungen</span>
			<button
				type="button"
				class="button gelesen"
				disabled={gesperrt}
				aria-label="Gelesen: {e.titel}"
				onclick={(ev) => {
					ongelesen?.(e, ev.currentTarget);
				}}>gelesen</button
			>
		</div>
	{:else}
		<div class="handlung">
			<!-- Etappe 3b (Fund des Prüfers): sichtbare Beschriftung, Knöpfe mindestens 44 px. -->
			<div class="stufen" role="group" aria-labelledby="{kennung}-wann {kennung}-titel">
				<span class="wann" id="{kennung}-wann">Wann dran?</span>
				{#each STUFEN as s (s)}
					<button
						type="button"
						class="button klein"
						aria-pressed={gewaehlt === s}
						disabled={gesperrt}
						onclick={() => {
							onumstufen(e, s);
						}}>{STUFE_WORT[s]}</button
					>
				{/each}
				<button
					type="button"
					class="button klein vorschlag"
					disabled={gesperrt || e.stufe.inhaber === null}
					title="Zurück zu dem, was der Chat vorgeschlagen hat"
					onclick={() => {
						onumstufen(e, null);
					}}>Wie vorgeschlagen ({STUFE_WORT[e.stufe.vorschlag]})</button
				>
			</div>
			<!-- Eine Aufgabe beim Chat (eigene Worte geschickt) wartet nicht auf dich: keine Knöpfe. -->
			{#if e.art === 'aufgabe' && e.zustand !== ZUSTAND_BEIM_CHAT}
				<div class="haken">
					{#if e.knopf}
						{#each e.knopf as k (k)}
							<button
								type="button"
								class="button"
								disabled={gesperrt}
								aria-label="{k}: {e.titel}"
								onclick={() => {
									onhaken(e.id, k);
								}}>{k}</button
							>
						{/each}
					{:else}
						<button
							type="button"
							class="button"
							disabled={gesperrt}
							aria-label="Erledigt: {e.titel}"
							onclick={() => {
								onhaken(e.id, null);
							}}><span aria-hidden="true">✓</span> erledigt</button
						>
					{/if}
					<EtwasAnderes
						entwurf={aufgabeEntwurf(e.id)}
						bezug={e.titel}
						{gesperrt}
						senden={(eigene: string) => oneigene(e.id, eigene)}
					/>
				</div>
			{/if}
		</div>
	{/if}
</li>

<style>
	.eintrag {
		display: grid;
		gap: 0.35rem;
		padding: 0.7rem 0.85rem;
		border: 1px solid var(--line);
		border-left: 0.25rem solid var(--line);
		border-radius: calc(var(--radius) - 4px);
		background: var(--card);
	}
	.eintrag.jetzt {
		border-left-color: var(--warn);
	}
	.kopf {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.25rem 0.6rem;
	}
	.art {
		flex: none;
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--muted);
		min-width: 5.5rem;
	}
	.titel {
		flex: 1 1 16rem;
		margin: 0;
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.meta {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0 0.35rem;
	}
	.frist.achtung {
		font-weight: 700;
		color: var(--text);
		background: color-mix(in srgb, var(--warn) 30%, transparent);
		padding: 0 0.35rem;
		border-radius: 0.35rem;
	}
	.vermerke {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}
	.handlung {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 0.5rem 1rem;
		margin-top: 0.15rem;
	}
	.stufen,
	.haken {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
	}
	.wann {
		margin-right: 0.2rem;
		font-size: var(--text-sm);
		color: var(--muted);
	}
	.klein {
		padding: 0.2rem 0.65rem;
		font-size: var(--text-sm);
		min-width: 2.75rem;
		min-height: 2.75rem;
	}
	.vorschlag {
		color: var(--muted);
	}
	.text {
		margin: 0;
		overflow-wrap: anywhere;
		white-space: pre-line;
	}
	.gelesen {
		padding: 0.2rem 0.75rem;
		min-height: 2.75rem;
	}
	.button:disabled {
		opacity: 0.6;
		cursor: not-allowed;
	}
	/* Prüfbericht: Der Knopf ist meist gesperrt (die Stufe ist schon
	   die vorgeschlagene). --muted mit Deckkraft 0,6 hatte nur 2,6:1; ohne
	   Deckkraft 5,8:1 (kontrast.spec.ts). Gesperrt zeigt der gestrichelte
	   Rand, dazu cursor und disabled. */
	.vorschlag:disabled {
		opacity: 1;
		border-style: dashed;
	}
	@media (prefers-reduced-motion: reduce) {
		.button {
			transition: none;
		}
	}
</style>
