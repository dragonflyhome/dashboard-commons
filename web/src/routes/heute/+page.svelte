<script lang="ts">
	/**
	 * Seite /heute (Kalender, Stufe 7b): Uhr, Sonnenbogen und Mond am Blickort, oben
	 * die Wahl „Ich bin gerade in“. Die Daten kommen aus dem gemeinsamen Himmel-Laden
	 * (`himmel.svelte.ts`), den diese Seite „häufig“ nutzt (alle 10 Minuten neu, für
	 * die Sonnenhöhe); die Uhr hat ihren eigenen Minutentakt.
	 *
	 * Die Wahl wirkt sofort (`POST /api/kalender/blickort`); „Zuhause (…)“ schickt
	 * `null`, der Blickort folgt dann dem Heimatort. Ein Treffer der Städtesuche wird
	 * erst gemerkt (`POST /api/kalender/standorte`, den Abgleich mit schon gemerkten
	 * Orten macht der Server) und dann gewählt. Koordinaten gehen nur im Rumpf, nie
	 * in die Adresse und nie in den Speicher des Browsers.
	 */
	import { onMount } from 'svelte';
	import { resolve } from '$app/paths';
	import { Abschnitt, Absatz, Auswahl } from '$katalog';
	import { SATZ_ANMELDUNG_NOETIG } from '#lib/anmeldung.ts';
	import { API, ApiError, fehlerText, getJson, senden } from '#lib/api/client.ts';
	import { fmtDate } from '#lib/format.ts';
	import { himmel } from '#lib/himmel.svelte.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import Mond from '#lib/components/kalender/Mond.svelte';
	import Sonnenbogen from '#lib/components/kalender/Sonnenbogen.svelte';
	import Staedtesuche from '#lib/components/kalender/Staedtesuche.svelte';
	import Uhr from '#lib/components/kalender/Uhr.svelte';
	import type { KalenderMerken, KalenderOrt, KalenderOrte, KalenderStadt } from '$vertrag';

	/** Der Satz, wenn der Blickort keine IANA-Zone hat (Rechnerzeit oder fester Versatz). */
	const SATZ_OHNE_ZONE =
		'Für diesen Ort ist keine Zeitzone hinterlegt; die Uhr zeigt die Zeit des Rechners. Über die Städtesuche bekommt er eine.';

	let orte = $state<KalenderOrt[] | null>(null);
	let meldung = $state<string | null>(null);
	let uhrzeit = $state('');

	async function orteLaden(): Promise<void> {
		try {
			orte = (await getJson<KalenderOrte>(`${API}/kalender/standorte`)).orte;
		} catch (e) {
			meldung = fehlerText(e);
		}
	}

	onMount(() => {
		void orteLaden();
		return himmel.nutzen(true);
	});

	const k = $derived(himmel.kopf);
	const fehler = $derived(himmel.fehler);
	const ohneAnmeldung = $derived(
		fehler instanceof ApiError && (fehler.status === 401 || fehler.art === 'fremdes-konto')
	);
	const ohneOrt = $derived(fehler instanceof ApiError && fehler.status === 404);

	const optionen = $derived.by(() => {
		const heimat = orte?.find((o) => o.heimat)?.titel ?? k?.ort ?? '';
		return [
			{ wert: '', text: `Zuhause (${heimat})` },
			...(orte ?? []).filter((o) => !o.heimat).map((o) => ({ wert: o.kennung, text: o.titel }))
		];
	});
	// Die Auswahl kennt nur Text; '' steht für „folgt dem Heimatort“ (keine Kennung ist leer).
	const gewaehlt = $derived(k === null || k.folgt_heimat ? '' : k.kennung);

	async function waehlen(ort: string | null): Promise<void> {
		meldung = null;
		try {
			await himmel.waehlen(ort);
		} catch (e) {
			meldung = fehlerText(e);
		}
	}

	/** Merkt die Stadt (der Server gibt einen schon gemerkten gleichen Ort zurück) und wählt sie. */
	async function stadtGewaehlt(s: KalenderStadt, text: string): Promise<void> {
		meldung = null;
		const zone = s.zeitzone.trim();
		const koerper: KalenderMerken = {
			// Der Server nimmt höchstens 60 Zeichen.
			titel: Array.from(text.trim()).slice(0, 60).join(''),
			breite: s.breite,
			laenge: s.laenge,
			zeitzone: zone === '' ? null : zone
		};
		try {
			const neu = await senden<KalenderOrt>(`${API}/kalender/standorte`, koerper);
			void orteLaden();
			await himmel.waehlen(neu.kennung);
		} catch (e) {
			// Scheitert die Wahl, bleibt der gemerkte Ort trotzdem in der Auswahl.
			meldung = fehlerText(e);
		}
	}
</script>

<svelte:head>
	<title>{seitenTitel('Heute')}</title>
</svelte:head>

<h1>Heute</h1>

{#if ohneAnmeldung}
	<section class="card" data-ohne-anmeldung>
		<p>{SATZ_ANMELDUNG_NOETIG}</p>
		<p><a href={resolve('/anmelden')}>Zur Anmeldung</a></p>
	</section>
{:else if k === null && fehler === null}
	<p class="muted small" aria-busy="true">Der Himmel wird geladen …</p>
{:else}
	<Abschnitt titel="Ich bin gerade in">
		{#if k !== null}
			<Auswahl
				beschriftung="Ort"
				{optionen}
				wert={gewaehlt}
				gesperrt={himmel.waehlt}
				onwechsel={(w: string) => void waehlen(w === '' ? null : w)}
			/>
		{:else if ohneOrt}
			<p data-ohne-ort>{fehlerText(fehler)}</p>
		{/if}
		<Staedtesuche onwahl={stadtGewaehlt} onfehler={(satz: string) => (meldung = satz)} />
		<Absatz>Eine gewählte Stadt wird als Ort gemerkt; hier wird nichts umgezogen.</Absatz>
	</Abschnitt>

	{#if meldung !== null}
		<p class="block-error" role="alert">{meldung}</p>
	{/if}
	{#if k === null && !ohneOrt}
		<p class="block-error" role="alert" data-fehler>{fehlerText(fehler)}</p>
	{/if}

	{#if k !== null}
		<Abschnitt titel="Uhr">
			<Uhr datum={k.datum} ort={k.ort} onuhrzeit={(t: string) => (uhrzeit = t)} />
			{#if k.zone === null}
				<p class="muted small" data-ohne-zone>{SATZ_OHNE_ZONE}</p>
			{/if}
		</Abschnitt>

		<Abschnitt titel="Sonne">
			{#if uhrzeit !== ''}
				<Sonnenbogen kopf={k} {uhrzeit} />
			{/if}
		</Abschnitt>

		<Abschnitt titel="Mond">
			<div class="mond">
				<Mond kopf={k} groesse={96} name="Mond: {k.phase}" />
				<p>
					Phase: {k.phase}<br />Nächster Vollmond: {fmtDate(k.naechster_vollmond)}
				</p>
			</div>
		</Abschnitt>
	{/if}
{/if}

<style>
	.mond {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 1rem;
	}
	.mond p {
		margin: 0;
	}
</style>
