<script lang="ts">
	/**
	 * Der Live-Punkt oben rechts (Kopfleiste, Fassung 2): ein Link zum
	 * Abschnitt „Dashboard“ auf /pc (Live-Kanal, Bote, Fassung, Ordner), mit
	 * einem der fünf Zustände als Text daneben. Der Zustand kommt aus dem
	 * freien Kanal (`freikanal.svelte.ts`), zugeordnet in `verbindung.ts`.
	 * Solange der erste Versuch läuft, steht nur der Punkt da.
	 *
	 * Daten live, App-Fassung getrennt, und
	 * „ich sollte gar nichts anklicken müssen“. Darum steht hier höchstens
	 * einer von drei Zuständen (`fassung.lage`, Vorrang 1 vor 3 vor 2), als
	 * Information, ohne Warnfarbe (auch der Punkt ist dann grau):
	 *
	 * 1. „Keine Verbindung zum Dienst, Daten können veraltet sein“ (Link wie
	 *    sonst; Kanal weg oder die API antwortet nicht, `apiStand`);
	 * 3. „Daten live, neue Fassung wird gebaut“ (Link wie sonst; ohne Anmeldung
	 *    aus dem Bau-Stand der Projektliste);
	 * 2. „Daten live, App wird gleich aktualisiert“, solange ein Sperrgrund
	 *    aufhält „App wird aktualisiert, sobald du fertig bist“: ein Knopf, die
	 *    Seite lädt von selbst (`FassungWache.svelte`), der Klick lädt sofort,
	 *    wenn nichts offen ist (`fassung.jetztLaden`).
	 *
	 * Jeder Wechsel des Zustands wird einmal angesagt (die Region darunter),
	 * ebenso das Umschalten auf „sobald du fertig bist“ (nicht das Zurück);
	 * dazwischen und nach dem Neuladen nicht. Wechseln Knopf und Link, wandert
	 * der Tastaturfokus mit. Ohne Anmeldung dieselben Texte (sie zeigen keine
	 * Daten, keinen Satz aus dem Bau und keinen Schrittzähler).
	 *
	 * Die Props nur für Tests; sonst der freie Kanal und `fassung`.
	 */
	import { resolve } from '$app/paths';
	import { untrack } from 'svelte';
	import { apiStand } from '#lib/api/antwortet.svelte.ts';
	import { fassung } from '#lib/fassung.svelte.ts';
	import {
		LAGE_TEXT,
		TEXT_AUFGEHALTEN,
		TITEL_APP_VERALTET,
		lageVon,
		type Lage
	} from '#lib/fassung.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import {
		LIVE_TEXT,
		STAND_ANKER,
		istLive,
		liveFarbe,
		liveName,
		type LiveZustand
	} from '#lib/verbindung.ts';

	interface Props {
		zustand?: LiveZustand | null;
		veraltet?: boolean;
		baut?: boolean;
		aufgehalten?: boolean;
	}
	let {
		zustand: vorgegeben,
		veraltet: vorgegebenVeraltet,
		baut: vorgegebenBaut,
		aufgehalten: vorgegebenAufgehalten
	}: Props = $props();

	const zustand = $derived(vorgegeben === undefined ? freierKanal.zustand : vorgegeben);
	const lage: Lage | null = $derived(
		lageVon(
			zustand === null ? null : istLive(zustand) && apiStand.antwortet,
			vorgegebenBaut ?? fassung.baut,
			vorgegebenVeraltet ?? fassung.veraltet
		)
	);
	// Zustand 1 ohne Warnfarbe: der Punkt grau wie beim ersten Versuch.
	const farbe = $derived(
		zustand === null || lage === 'keine_verbindung' ? 'sucht' : liveFarbe(zustand)
	);
	const aufgehalten = $derived(vorgegebenAufgehalten ?? fassung.aufgehalten);
	const text = $derived(
		lage === null ? null : lage === 'veraltet' && aufgehalten ? TEXT_AUFGEHALTEN : LAGE_TEXT[lage]
	);
	const name = $derived(
		text !== null
			? `${text}, Stand ansehen`
			: zustand === null
				? 'Verbindung wird geprüft, Stand ansehen'
				: liveName(zustand)
	);

	// Jeder Wechsel des Zustands einmal: neuer Satz in der Region. Ohne Zustand
	// wird sie leer, damit derselbe Zustand beim nächsten Mal wieder angesagt wird.
	// In Zustand 2 auch das Umschalten auf „aufgehalten“, das Zurückschalten nicht:
	// Dann wird sie still leer, damit das nächste Umschalten wieder angesagt wird.
	let ansage = $state('');
	let angesagt: Lage | null = null;
	let warAufgehalten = false;
	$effect(() => {
		const l = lage;
		const auf = l === 'veraltet' && aufgehalten;
		if (l !== angesagt) {
			angesagt = l;
			ansage = l === null ? '' : (untrack(() => text) ?? '');
		} else if (auf && !warAufgehalten) ansage = TEXT_AUFGEHALTEN;
		else if (!auf && warAufgehalten) ansage = '';
		warAufgehalten = auf;
	});

	// Knopf und Link sind zwei Elemente; beim Wechsel wandert der Fokus mit,
	// statt auf body zu fallen. `pre` misst vor dem Tausch, `$effect` danach.
	let ort: HTMLSpanElement | undefined = $state();
	let fokusDrin = false;
	$effect.pre(() => {
		const soll = lage === 'veraltet' ? 'BUTTON' : 'A';
		const da = ort?.querySelector('.live');
		fokusDrin = da != null && da.tagName !== soll && ort?.contains(document.activeElement) === true;
	});
	$effect(() => {
		const ziel = ort?.querySelector<HTMLElement>(lage === 'veraltet' ? 'button.live' : 'a.live');
		if (fokusDrin) ziel?.focus();
		fokusDrin = false;
	});
</script>

<span class="ort" data-live-punkt bind:this={ort}>
	{#if lage === 'veraltet'}
		<!-- Name ist der sichtbare Text; der Tooltip sagt, was geschieht. -->
		<button
			type="button"
			class="button live {farbe} hinweis"
			title={TITEL_APP_VERALTET}
			data-zustand={zustand ?? 'sucht'}
			data-lage={lage}
			onclick={() => {
				fassung.jetztLaden();
			}}
		>
			<span class="pulse" aria-hidden="true"></span>
			<span class="text">{text}</span>
		</button>
	{:else}
		<!-- eslint-disable svelte/no-navigation-without-resolve -- baut mit resolve(), dazu die Sprungmarke -->
		<a
			class="live {farbe}"
			class:hinweis={lage !== null}
			href={`${resolve('/pc')}#${STAND_ANKER}`}
			aria-label={name}
			title={name}
			data-zustand={zustand ?? 'sucht'}
			data-lage={lage ?? undefined}
		>
			<span class="pulse" aria-hidden="true"></span>
			{#if zustand !== null}
				<span class="text" aria-hidden="true">{text ?? LIVE_TEXT[zustand]}</span>
			{/if}
		</a>
		<!-- eslint-enable svelte/no-navigation-without-resolve -->
	{/if}
	<!-- Immer da (leer ohne Zustand), damit jeder Wechsel einmal angesagt wird. -->
	<span class="nur-vorlesen" role="status">{ansage}</span>
</span>

<style>
	/* Bezug für die Ansage-Region (absolut), damit nichts die Kopfleiste verbreitert. */
	.ort {
		position: relative;
		display: inline-flex;
		align-items: center;
	}
	.live {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		min-height: 2.75rem;
		min-width: 2.75rem;
		padding: 0 0.5rem;
		border-radius: 0.625rem;
		font-size: var(--text-sm);
		color: var(--muted);
		white-space: nowrap;
		text-decoration: none;
	}
	/* Zustand 2: der Knopf des Katalogs (class button), hier so hoch wie der Link. */
	button.live {
		margin: 0;
		font-size: var(--text-sm);
		text-align: left;
	}
	/* Die Zustände sind länger als „live, angemeldet“: umbrechen, nie überbreit. */
	.hinweis .text {
		max-width: 11rem;
		color: var(--text);
		line-height: 1.2;
		white-space: normal;
	}
	.live:hover .text {
		color: var(--text);
		text-decoration: underline;
	}
	.pulse {
		flex: none;
		width: 0.625rem;
		height: 0.625rem;
		border-radius: 50%;
		background: var(--muted);
	}
	.ok .pulse {
		background: var(--ok);
		animation: pulse 2.4s ease-out infinite;
	}
	@keyframes pulse {
		0% {
			box-shadow: var(--puls);
		}
		70% {
			box-shadow: var(--puls-weit);
		}
		100% {
			box-shadow: none;
		}
	}
	/* Handy: der Text bricht in kurze Zeilen, damit der Titel der Seite
	   in der ersten Zeile Platz behält (er ist dort die einzige Ortsangabe). */
	@media (max-width: 37.5rem) {
		.live {
			padding: 0 0.25rem;
			gap: 0.375rem;
		}
		.text,
		.hinweis .text {
			max-width: 6.5rem;
			font-size: var(--text-xs);
			line-height: 1.15;
			white-space: normal;
		}
		.hinweis .text {
			max-width: 8.5rem;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.ok .pulse {
			animation: none;
		}
	}
</style>
