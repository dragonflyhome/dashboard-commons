<script lang="ts">
	/**
	 * Brotkrumen links in der Kopfleiste (Fassung 3): das Symbol als
	 * Wurzel (Startseite), dann je Ebene ein Glied, zuletzt das Blatt (die
	 * aktuelle Seite). Das Blatt ist ein Knopf: Er prüft mit dem Server, ob die
	 * Daten aktuell sind, zieht nach, was fehlt (ohne neue Browserseite), und
	 * scrollt nach oben (`aktualitaet.svelte.ts`). Am Handy (unter 37,5 rem)
	 * stehen die mittleren Glieder als „…“ da; Vorlese-Programme hören sie
	 * trotzdem, Symbol und Blatt bleiben.
	 *
	 * Als Edge-App ohne Adressleiste ist das Blatt die einzige Ortsangabe;
	 * zu lang wird es mit Auslassungspunkten gekürzt (ganzer Weg im Titel).
	 * Gekürzt werden nur Glieder und Blatt (ganzer Text im title und für
	 * Vorlese-Programme im Text); Symbol und „…“ behalten ihre Breite, sonst
	 * ragten sie bei einem langen Projekttitel über den Rand (360 px).
	 */
	import { asset, resolve } from '$app/paths';
	import { ABGLEICH_SATZ, type Abgleich } from '#lib/aktualitaet.svelte.ts';
	import { PROJEKTE_ANKER, type Glied, type Kopf } from '#lib/kopfleiste.ts';

	interface Props {
		kopf: Kopf;
		/** Läuft gerade ein Abgleich? */
		laeuft?: boolean;
		/** Das letzte Ergebnis (kurz), sonst `null`. */
		ergebnis?: Abgleich | null;
		/** Klick auf das Blatt: abgleichen und nachziehen. */
		onblatt: () => Promise<unknown>;
	}
	let { kopf, laeuft = false, ergebnis = null, onblatt }: Props = $props();
	const id = $props.id();

	/** Was das Blatt tut, für Vorlese-Programme (Auflage aus der Abnahme). */
	const BLATT_BESCHREIBUNG = 'Seite auf den neuesten Stand bringen und nach oben';

	function adresse(g: Glied): string {
		switch (g.ziel.art) {
			case 'projekte':
				return `${resolve('/')}#${PROJEKTE_ANKER}`;
			case 'projekt':
				return resolve('/projects/[slug]', { slug: g.ziel.slug });
			case 'gruppe':
				return resolve('/gruppen/[slug]', { slug: g.ziel.slug });
			case 'katalog':
				return resolve('/katalog');
		}
	}

	const satz = $derived(ergebnis === null ? '' : ABGLEICH_SATZ[ergebnis]);

	async function blatt(): Promise<void> {
		const reduziert = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		// Gleich nach oben; das Nachziehen läuft, während die Seite rollt.
		window.scrollTo({ top: 0, behavior: reduziert ? 'auto' : 'smooth' });
		await onblatt();
	}
</script>

<nav class="krumen" aria-label="Brotkrumen" title={kopf.titel}>
	<ol>
		<li class="wurzel">
			<!-- Das Symbol führt zur Startseite; der Ring heißt „hier“ (Startseite). -->
			<a class="symbol" class:hier={kopf.zuhause} href={resolve('/')} aria-label="Startseite">
				<!-- Das Logo: eine Datei für alle Stellen (static/logo.svg, Etappe 3b-2). -->
				<img src={asset('logo.svg')} alt="" width="28" height="28" />
			</a>
		</li>
		{#if kopf.glieder.length > 0}
			<li class="auslassung">
				<span aria-hidden="true">…</span><span class="nur-vorlesen"
					>{kopf.glieder.map((g) => g.text).join(' › ')}</span
				>
			</li>
		{/if}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- adresse() baut mit resolve(), dazu die Sprungmarke -->
		{#each kopf.glieder as g (g.text + g.ziel.art)}
			<li class="mitte"><a href={adresse(g)} title={g.text}>{g.text}</a></li>
		{/each}
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
		<li class="blatt">
			<button
				type="button"
				aria-current="page"
				aria-busy={laeuft}
				aria-describedby="{id}-blatt"
				class:geprueft={ergebnis !== null && ergebnis !== 'ohne-server'}
				title="{kopf.blatt}: prüfen, ob alles aktuell ist, und nach oben"
				onclick={() => void blatt()}
				><span class="text">{kopf.blatt}</span><span class="zeichen" aria-hidden="true"
					>{laeuft ? '…' : ergebnis !== null && ergebnis !== 'ohne-server' ? '✓' : ''}</span
				></button
			>
			<span class="nur-vorlesen" id="{id}-blatt">{BLATT_BESCHREIBUNG}</span>
		</li>
	</ol>
	<!-- Das Ergebnis des Abgleichs höflich ansagen (und im Titel des Blatts). -->
	<p class="nur-vorlesen" aria-live="polite">{satz}</p>
</nav>

<style>
	.krumen {
		/* Bezug für den unsichtbaren Satz (position: absolute). */
		position: relative;
		min-width: 0;
	}
	ol {
		display: flex;
		align-items: center;
		min-width: 0;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: flex;
		align-items: center;
		min-width: 0;
	}
	/* Symbol und „…“ schrumpfen nie: Platz gibt nur ab, was sich kürzen lässt. */
	.wurzel,
	.auslassung {
		flex: none;
	}
	/* Trenner zwischen den Gliedern (nicht vor dem Symbol). */
	li + li::before {
		content: '›';
		flex: none;
		padding: 0 0.375rem;
		color: var(--muted);
	}
	.symbol {
		display: flex;
		flex: none;
		align-items: center;
		justify-content: center;
		width: 2.75rem;
		height: 2.75rem;
		border-radius: 0.625rem;
	}
	.symbol img {
		display: block;
		width: 1.75rem;
		height: 1.75rem;
		border-radius: 0.5rem;
		animation: spin-in 700ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	/* „Hier“: auf der Startseite. */
	.symbol.hier img {
		box-shadow: var(--ring-akzent);
	}
	@keyframes spin-in {
		from {
			transform: rotate(-90deg) scale(0.6);
			opacity: 0;
		}
	}
	/* Wird es eng, kürzen sich zuerst die mittleren Glieder (nie unter 44 px). */
	.mitte {
		flex: 0 3 auto;
		/* Trenner und mindestens 44 px Link: nie schmaler, sonst überlappen die Glieder. */
		min-width: 4.25rem;
	}
	.mitte a {
		display: block;
		flex: 0 1 auto;
		min-width: 2.75rem;
		line-height: 2.75rem;
		overflow: hidden;
		color: var(--muted);
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.mitte a:hover {
		color: var(--text);
	}
	/* „…“ nur am Handy. */
	.auslassung {
		display: none;
		color: var(--muted);
	}
	.blatt {
		flex: 0 1 auto;
	}
	.blatt button {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		min-width: 2.75rem;
		min-height: 2.75rem;
		max-width: 100%;
		padding: 0 0.375rem;
		border: 0;
		border-radius: 0.625rem;
		background: none;
		color: var(--text);
		font: inherit;
		font-size: var(--text-lg);
		font-weight: 700;
		cursor: pointer;
	}
	.blatt button:hover {
		background: var(--accent-soft);
	}
	.blatt .text {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.zeichen {
		flex: none;
		font-size: var(--text-sm);
		font-weight: 400;
		color: var(--ok-text);
	}
	@media (max-width: 37.5rem) {
		/* Der Weg steht für Vorlese-Programme im „…“; die Links selbst fehlen,
		   damit die Tastatur nicht auf Unsichtbares springt. */
		.mitte {
			display: none;
		}
		.auslassung {
			position: relative;
			display: flex;
		}
		.blatt button {
			font-size: var(--text-base);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.symbol img {
			animation: none;
		}
	}
</style>
