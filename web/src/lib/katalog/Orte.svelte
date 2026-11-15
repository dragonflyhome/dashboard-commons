<script lang="ts">
	/**
	 * Orte aus `daten/orte.toml`: je Datei oder Ordner auf diesem PC Titel,
	 * Hinweis und ein Knopf „Öffnen“ (Entscheidung lokal-oeffnen).
	 * Der Knopf schickt nur Projekt und Kennung. Kein „Pfad kopieren“.
	 * Das Projekt kommt aus der Route wie bei `ladeDaten`.
	 *
	 * Schritt E1b: Angemeldet holt der Baustein die Orte mit Pfad vom
	 * geschützten Weg (`ladeOrte`) und zeigt den Pfad eines geprüften Orts als
	 * Text und als `file:`-Link. Browser folgen ihm aus einer http-Seite nicht;
	 * folgte einer doch (Richtlinie), ginge er an Server und Bote vorbei. Darum
	 * fangen Klick, Mittelklick und Enter ab und öffnen nur über
	 * `POST /api/oeffnen`. Ohne Anmeldung und nach dem Abmelden (auch Widerruf,
	 * Ablauf) kein Pfad, wie bisher nur Titel und Knopf.
	 */
	import type { Abgeleitet, Orte, OrtVoll } from '$vertrag';
	import Daten from './intern/Daten.svelte';
	import type { Quelle } from './abgeleitet.ts';
	import { kontext } from '../projekt/laden.svelte.ts';
	import { dateiAdresse, fehlerSatz, ladeOrte, oeffnen } from '../oeffnen.ts';
	import { freierKanal } from '../freikanal.svelte.ts';

	interface Props {
		daten: Quelle<Orte>;
		teil?: string | undefined;
		abgeleitet?: Abgeleitet | null | undefined;
		/** Nur für Beispiele und Tests; sonst das Projekt der Seite. */
		projekt?: string | undefined;
		/** Nur für Tests. */
		fetchFn?: typeof fetch | undefined;
	}
	let { daten, teil, abgeleitet, projekt, fetchFn }: Props = $props();

	/** Ohne `projekt` gilt das Projekt der Route (Kontext, nur beim Anlegen lesbar). */
	function projektDerSeite(): { readonly slug: string } | null {
		return projekt === undefined ? kontext('Orte') : null;
	}
	const seite = projektDerSeite();
	const slug = $derived(projekt ?? seite?.slug ?? '');

	/** Je Kennung: läuft gerade, oder der letzte Satz (geöffnet oder warum nicht). */
	let laeuft = $state<Record<string, boolean>>({});
	let saetze = $state<Record<string, { text: string; gut: boolean }>>({});

	/** Je Kennung der Ort mit Pfad vom geschützten Weg (leer ohne Anmeldung). */
	let mitPfad = $state<Record<string, OrtVoll>>({});
	let anfrage = 0;

	async function pfadeLaden(s: string): Promise<void> {
		const nr = ++anfrage;
		const o = await ladeOrte(s, fetchFn ?? ((...a) => fetch(...a)));
		if (nr === anfrage) mitPfad = Object.fromEntries((o?.orte ?? []).map((x) => [x.kennung, x]));
	}

	$effect(() => {
		const s = slug;
		if (s === '') return;
		// Projektwechsel ohne eigene Seite: keine Pfade des alten Projekts an gleichen Kennungen.
		mitPfad = {};
		void pfadeLaden(s);
		const ab = freierKanal.onAbgemeldet(() => {
			// Sofort weg, dann neu fragen (ohne Anmeldung kommt nichts).
			anfrage += 1;
			mitPfad = {};
			void pfadeLaden(s);
		});
		// Angemeldet ohne Neuladen (Kanalwechsel frei → geschützt): jetzt die Pfade holen.
		const an = freierKanal.onBereit((k) => {
			if (k === 'geschuetzt') void pfadeLaden(s);
		});
		return () => {
			ab();
			an();
		};
	});

	async function auf(kennung: string): Promise<void> {
		laeuft[kennung] = true;
		try {
			const a = await oeffnen(slug, kennung, fetchFn ?? fetch);
			saetze[kennung] = { text: a.satz, gut: true };
		} catch (e) {
			saetze[kennung] = { text: fehlerSatz(e), gut: false };
		} finally {
			laeuft[kennung] = false;
		}
	}

	/** Der Link folgt nie dem `href`: Klick und Enter öffnen über den Server. */
	function abfangen(ev: Event, kennung: string): void {
		ev.preventDefault();
		if (laeuft[kennung] !== true) void auf(kennung);
	}
</script>

<Daten {daten} {teil} {abgeleitet}>
	{#snippet children(o: Orte)}
		{#if o.ort.length === 0}
			<p class="muted small">Noch keine Orte in <code>daten/orte.toml</code>.</p>
		{:else}
			<ul class="plain orte">
				{#each o.ort as ort (ort.kennung)}
					{@const voll = mitPfad[ort.kennung]}
					{@const adresse = voll ? dateiAdresse(voll) : null}
					<li>
						<div class="text">
							<span class="titel">{ort.titel}</span>
							{#if ort.hinweis}
								<span class="muted small">{ort.hinweis}</span>
							{/if}
							{#if voll?.pfad && adresse !== null}
								<!-- eslint-disable svelte/no-navigation-without-resolve -- file:-Adresse eines geprüften Orts außerhalb der App; Klick, Mittelklick und Enter fangen ab und öffnen über POST /api/oeffnen -->
								<a
									class="pfad small"
									href={adresse}
									onclick={(ev) => {
										abfangen(ev, ort.kennung);
									}}
									onauxclick={(ev) => {
										// Nur die mittlere Taste; die rechte öffnet das Menü des Browsers.
										if (ev.button !== 1) return;
										abfangen(ev, ort.kennung);
									}}
									onkeydown={(ev) => {
										if (ev.key === 'Enter') abfangen(ev, ort.kennung);
									}}><code>{voll.pfad}</code></a
								>
								<!-- eslint-enable svelte/no-navigation-without-resolve -->
							{:else if voll?.pfad}
								<code class="pfad small">{voll.pfad}</code>
							{/if}
						</div>
						<button
							type="button"
							class="button"
							disabled={laeuft[ort.kennung] === true}
							aria-label="{ort.titel} öffnen"
							onclick={() => void auf(ort.kennung)}
						>
							Öffnen
						</button>
						{#if saetze[ort.kennung]}
							<p
								class="satz small"
								class:block-error={!saetze[ort.kennung]?.gut}
								role={saetze[ort.kennung]?.gut ? 'status' : 'alert'}
							>
								{saetze[ort.kennung]?.text}
							</p>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	{/snippet}
</Daten>

<style>
	.orte li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 6px 12px;
		padding: 8px 0;
		border-bottom: 1px solid var(--line);
	}
	.orte li:last-child {
		border-bottom: none;
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.satz {
		flex-basis: 100%;
		margin: 0;
	}
</style>
