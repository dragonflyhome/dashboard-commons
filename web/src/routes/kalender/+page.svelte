<script lang="ts">
	/**
	 * Seite /kalender: Tageszeiten, laufender Mond und die Feste der nächsten
	 * 30 Tage für einen benannten Ort (der Heimatort zuerst) oder für einen
	 * anderen Ort aus Breite und Länge. Alles rechnet der Server; die Seite
	 * zeigt nur.
	 *
	 * Datenschutz: Die Koordinaten eines anderen Orts gehen nur im Rumpf von
	 * `POST /api/kalender/abfrage` (und, nur auf Klick auf „Ort merken“, von
	 * `POST /api/kalender/standorte`), nie in die Adresse, nie in den
	 * Speicher des Browsers und nie in die Konsole. Ungemerkt leben sie nur im
	 * Zustand dieser Seite und sind weg, sobald man sie verlässt. Die
	 * Städtesuche schickt den Suchtext ebenso nur im Rumpf von
	 * `POST /api/kalender/staedte`.
	 */
	import { onMount, tick } from 'svelte';
	import { resolve } from '$app/paths';
	import { Abschnitt, Absatz, Auswahl, Hinweis, Knopf } from '$katalog';
	import { API, fehlerText, getJson, readError } from '#lib/api/client.ts';
	import { abrufen } from '#lib/api/antwortet.svelte.ts';
	import { fmtDate } from '#lib/format.ts';
	import { nameKurz } from '#lib/fristen.ts';
	import { lokalerTag } from '#lib/tageswechsel.ts';
	import { tagPlus } from '#lib/zeit/tage.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import type {
		KalenderAbfrage,
		KalenderAbfrageAntwort,
		KalenderFeste,
		KalenderMerken,
		KalenderOrt,
		KalenderOrte,
		KalenderStadt,
		KalenderStaedte,
		KalenderStaedteAnfrage,
		KalenderTag,
		KalenderZonen,
		Zeitraum
	} from '$vertrag';

	/** Die Feste ab heute, so viele Tage weit. */
	const FESTE_TAGE = 30;

	const heute = lokalerTag(new Date());
	const festeBis = tagPlus(heute, FESTE_TAGE);

	let orte = $state<KalenderOrt[] | null>(null);
	let ort = $state('');
	let datum = $state(heute);
	let tag = $state<KalenderTag | null>(null);
	let feste = $state<KalenderFeste | null>(null);
	let fehler = $state<string | null>(null);

	// „Anderer Ort“: Eingaben und der abgeschickte Ort, nur hier im Speicher.
	let breite = $state<number | null>(null);
	let laenge = $state<number | null>(null);
	let zeitzone = $state('');
	let frei = $state<{ breite: number; laenge: number; zeitzone: string | null } | null>(null);
	let gerechnet = $state<Pick<KalenderAbfrageAntwort, 'breite' | 'laenge' | 'naeherung'> | null>(
		null
	);

	// „Ort merken“: Titel und Rückmeldung.
	let merkTitel = $state('');
	let gemerkt = $state<string | null>(null);
	let gemerktFeld = $state<HTMLElement | null>(null);
	let zonen = $state<KalenderZonen | null>(null);

	// „Stadt suchen“: Suchtext und Antwort, nur hier im Speicher.
	let suchtext = $state('');
	let staedte = $state<KalenderStaedte | null>(null);
	let breiteFeld = $state<HTMLInputElement | null>(null);
	let suchNr = 0;

	async function stadtSuchen(): Promise<void> {
		const anfrage = suchtext.trim();
		if (anfrage === '') return;
		const nr = ++suchNr;
		const koerper: KalenderStaedteAnfrage = { anfrage, hoechstens: 10 };
		try {
			const a = await senden<KalenderStaedte>(`${API}/kalender/staedte`, koerper);
			if (nr === suchNr) staedte = a;
		} catch (e) {
			if (nr === suchNr) fehler = fehlerText(e);
		}
	}

	/** Enter im Suchfeld sucht, statt das Formular „Anderer Ort“ abzuschicken. */
	function suchTaste(e: KeyboardEvent): void {
		if (e.key !== 'Enter') return;
		e.preventDefault();
		void stadtSuchen();
	}

	/** Übernimmt die Stadt in Breite, Länge und Zeitzone; rechnen erst auf Knopfdruck. */
	function stadtWaehlen(s: KalenderStadt): void {
		breite = s.breite;
		laenge = s.laenge;
		zeitzone = s.zeitzone;
		staedte = null;
		breiteFeld?.focus();
	}

	const optionen = $derived(
		(orte ?? []).map((o) => ({ wert: o.kennung, text: o.heimat ? `${o.titel} (Heimat)` : o.titel }))
	);

	/** Lädt die Orte, der Heimatort zuerst; gewählt wird `waehlen` oder die Heimat. */
	async function orteLaden(waehlen: string | null): Promise<void> {
		const antwort = await getJson<KalenderOrte>(`${API}/kalender/standorte`);
		// Der Server liefert den Heimatort zuerst.
		ort = waehlen ?? antwort.orte[0]?.kennung ?? '';
		orte = antwort.orte;
	}

	/** POST mit JSON-Rumpf; Fehler wie bei `getJson`. */
	async function senden<T>(pfad: string, koerper: unknown): Promise<T> {
		const res = await abrufen(fetch, pfad, {
			method: 'POST',
			credentials: 'same-origin',
			headers: { accept: 'application/json', 'content-type': 'application/json' },
			body: JSON.stringify(koerper)
		});
		if (!res.ok) throw await readError(res);
		return (await res.json()) as T;
	}

	onMount(() => {
		orteLaden(null).catch((e: unknown) => {
			fehler = fehlerText(e);
		});
		// Ohne Zonenliste bleibt das Feld frei beschreibbar; kein Fehler auf der Seite.
		getJson<KalenderZonen>(`${API}/kalender/zonen`).then(
			(z) => (zonen = z),
			() => (zonen = null)
		);
	});

	/** Nur die letzte Anfrage zählt; eine langsamere ältere überschreibt nichts. */
	let laufend = 0;

	async function laden(
		d: string,
		o: string,
		f: { breite: number; laenge: number; zeitzone: string | null } | null
	): Promise<void> {
		const nr = ++laufend;
		try {
			if (f === null) {
				const ortTeil = o === '' ? '' : `&ort=${encodeURIComponent(o)}`;
				const [t, fe] = await Promise.all([
					getJson<KalenderTag>(`${API}/kalender/tag?datum=${encodeURIComponent(d)}${ortTeil}`),
					getJson<KalenderFeste>(`${API}/kalender/feste?von=${heute}&bis=${festeBis}${ortTeil}`)
				]);
				if (nr !== laufend) return;
				tag = t;
				feste = fe;
				gerechnet = null;
			} else {
				const koerper: KalenderAbfrage = {
					datum: d,
					von: heute,
					bis: festeBis,
					breite: f.breite,
					laenge: f.laenge,
					zeitzone: f.zeitzone
				};
				const a = await senden<KalenderAbfrageAntwort>(`${API}/kalender/abfrage`, koerper);
				if (nr !== laufend) return;
				tag = a.tag;
				feste = a.feste;
				gerechnet = { breite: a.breite, laenge: a.laenge, naeherung: a.naeherung };
			}
			fehler = null;
		} catch (e) {
			if (nr === laufend) fehler = fehlerText(e);
		}
	}

	$effect(() => {
		if (orte === null || datum === '') return;
		void laden(datum, ort, frei);
	});

	function abschicken(e: SubmitEvent): void {
		e.preventDefault();
		if (breite === null || laenge === null) return;
		const zone = zeitzone.trim();
		frei = { breite, laenge, zeitzone: zone === '' ? null : zone };
		gemerkt = null;
	}

	/** Speichert den gerechneten Ort unter dem Titel und wählt ihn danach aus. */
	async function merken(e: SubmitEvent): Promise<void> {
		e.preventDefault();
		const titel = merkTitel.trim();
		if (frei === null || titel === '') return;
		const koerper: KalenderMerken = { titel, ...frei };
		try {
			const neu = await senden<KalenderOrt>(`${API}/kalender/standorte`, koerper);
			vergessen();
			await orteLaden(neu.kennung);
			gemerkt = neu.titel;
			fehler = null;
			// Das Merken-Formular ist weg; der Fokus geht auf die Rückmeldung.
			await tick();
			gemerktFeld?.focus();
		} catch (err) {
			fehler = fehlerText(err);
		}
	}

	/** Zurück zum benannten Ort; der andere Ort ist danach vergessen. */
	function vergessen(): void {
		frei = null;
		breite = null;
		laenge = null;
		zeitzone = '';
		merkTitel = '';
	}

	const grad = (n: number): string => `${n.toFixed(1).replace('.', ',')}°`;
	const ortTitel = $derived(orte?.find((o) => o.kennung === ort)?.titel ?? 'meinem Ort');
</script>

<svelte:head>
	<title>{seitenTitel('Kalender')}</title>
</svelte:head>

{#snippet monat(r: Zeitraum)}
	<span class="small" data-zeitraum
		>Neumond {fmtDate(r.anfang)} bis {fmtDate(r.ende)},
		<strong>Vollmond {fmtDate(r.vollmond)}</strong></span
	>
{/snippet}

<p class="crumbs muted small"><a href={resolve('/')}>Übersicht</a> › Kalender</p>
<h1>Kalender</h1>

<Abschnitt titel="Ort und Tag">
	{#if orte !== null && orte.length > 0}
		<Auswahl beschriftung="Ort" {optionen} bind:wert={ort} onwechsel={vergessen} />
	{/if}
	<label class="feld">
		Tag
		<input type="date" bind:value={datum} required />
	</label>

	<form class="anders" onsubmit={abschicken} autocomplete="off">
		<fieldset>
			<legend>Anderer Ort</legend>
			<div class="suche">
				<label class="feld">
					Stadt suchen
					<input
						type="search"
						bind:value={suchtext}
						onkeydown={suchTaste}
						maxlength="100"
						spellcheck="false"
					/>
				</label>
				<Knopf onclick={() => void stadtSuchen()}>Suchen</Knopf>
			</div>
			<div role="status" data-staedte>
				{#if staedte !== null}
					{#if staedte.hinweis}
						<p class="muted small">{staedte.hinweis}</p>
					{:else if staedte.treffer.length === 0}
						<p class="muted small">Keine Stadt gefunden.</p>
					{:else}
						<ul class="plain treffer">
							{#each staedte.treffer as s, i (i)}
								<li>
									<Knopf
										art="leise"
										onclick={() => {
											stadtWaehlen(s);
										}}>{s.name}{s.region ? `, ${s.region}` : ''} ({s.land})</Knopf
									>
								</li>
							{/each}
						</ul>
						<p class="muted small" data-quelle>Städteliste: {staedte.quelle}</p>
					{/if}
				{/if}
			</div>
			<label class="feld">
				Breite (Grad, Norden positiv)
				<input
					type="number"
					bind:value={breite}
					bind:this={breiteFeld}
					min="-90"
					max="90"
					step="any"
					required
				/>
			</label>
			<label class="feld">
				Länge (Grad, Osten positiv)
				<input type="number" bind:value={laenge} min="-180" max="180" step="any" required />
			</label>
			<label class="feld">
				Zeitzone (freiwillig)
				<input
					type="text"
					bind:value={zeitzone}
					list="kalender-zonen"
					placeholder="leer: aus der Länge geschätzt"
					spellcheck="false"
				/>
			</label>
			<datalist id="kalender-zonen">
				{#each zonen?.zonen ?? [] as z (z)}<option value={z}></option>{/each}
			</datalist>
			<div class="knoepfe">
				<Knopf type="submit" art="haupt">Für diesen Ort rechnen</Knopf>
				{#if frei !== null}
					<Knopf art="leise" onclick={vergessen}>Zurück zu {ortTitel}</Knopf>
				{/if}
			</div>
		</fieldset>
	</form>
	<Absatz
		>Ein anderer Ort wird nur gespeichert, wenn du ihn merkst; sonst ist er weg, sobald du die Seite
		verlässt.</Absatz
	>
	{#if gerechnet !== null}
		<p data-gerechnet>
			Gerechnet für Breite {grad(gerechnet.breite)} und Länge {grad(gerechnet.laenge)}, auf ein
			Zehntelgrad gerundet.
		</p>
		{#if gerechnet.naeherung}
			<p class="muted small">
				Ohne Zeitzone ist die Uhrzeit aus der Länge geschätzt und kann etwas abweichen.
			</p>
		{/if}
		<form class="merken" onsubmit={merken} autocomplete="off">
			<label class="feld">
				Name für diesen Ort
				<input type="text" bind:value={merkTitel} minlength="1" maxlength="60" required />
			</label>
			<Knopf type="submit">Ort merken</Knopf>
		</form>
	{/if}
	<p role="status" data-gemerkt tabindex="-1" class:leer={gemerkt === null} bind:this={gemerktFeld}>
		{#if gemerkt !== null}Gemerkt als „{gemerkt}“.{/if}
	</p>
	{#if zonen !== null}
		<p class="muted small" data-tzdb>Zeitzonen-Daten {zonen.tzdb}</p>
	{/if}
</Abschnitt>

{#if fehler !== null}
	<p class="block-error" role="alert">{fehler}</p>
{/if}

{#if tag === null}
	{#if fehler === null}<p class="muted small" aria-busy="true">Der Kalender wird geladen …</p>{/if}
{:else}
	<Abschnitt titel="Tageszeiten am {fmtDate(tag.datum)}">
		{#if tag.hinweis}
			<Hinweis>{tag.hinweis}</Hinweis>
		{/if}
		<p class="muted small">
			{#if tag.ort}{tag.ort} ·
			{/if}Uhrzeiten in {tag.zeitart}
		</p>
		{#if tag.zeiten.length > 0}
			<ul class="plain zeilen">
				{#each tag.zeiten as z (z.kennung)}
					<li>
						<span class="wann"><b>{z.zeit ?? z.text ?? ''}</b></span>
						<span class="was">
							{z.name}
							<span class="muted small">{z.herkunft}</span>
						</span>
					</li>
				{/each}
			</ul>
		{/if}
		{#if tag.taglaenge}
			<p>Taglänge: {tag.taglaenge}</p>
		{/if}
		{#if tag.laengster_tag}
			<p><strong>{tag.laengster_tag}</strong></p>
		{/if}
	</Abschnitt>

	{#if tag.mond}
		{@const m = tag.mond}
		{@const erster = m.namen?.[0]}
		<Abschnitt titel="Laufender Mond">
			<p data-mond>
				{m.name}{#if erster}
					· {nameKurz(erster)}{/if}
				<span class="muted small">({m.kalender})</span>
				<br /><span data-phase>Phase: {m.phase}</span> · {@render monat(m.zeitraum)}
			</p>
			{#if m.hinweis}
				<Hinweis>{m.hinweis}</Hinweis>
			{/if}
		</Abschnitt>
	{/if}
{/if}

{#if feste !== null}
	<Abschnitt
		titel="Feste der nächsten {FESTE_TAGE} Tage"
		unterzeile="Vom {fmtDate(feste.von)} bis {fmtDate(feste.bis)}"
	>
		{#if feste.hinweis}
			<Hinweis>{feste.hinweis}</Hinweis>
		{/if}
		{#if feste.feste.length === 0}
			<p class="muted small">In dieser Zeit steht kein Fest an.</p>
		{:else}
			<ul class="plain zeilen">
				{#each feste.feste as f (`${f.kennung}-${f.datum}`)}
					<li>
						<span class="wann"><b>{fmtDate(f.datum)}</b></span>
						<span class="was">
							{f.titel}
							{#if f.zeitraum}{@render monat(f.zeitraum)}{/if}
							<span class="muted small">{f.kalender} · {f.herkunft}</span>
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	</Abschnitt>
{/if}

<style>
	.feld {
		display: grid;
		gap: 0.25rem;
		margin: 0.5rem 0;
		max-width: 20rem;
		font-size: var(--text-md);
	}
	.feld input {
		font: inherit;
		padding: 0.45rem 0.6rem;
		border: 1px solid var(--line);
		border-radius: 0.5rem;
		background: var(--bg);
		color: var(--text);
	}
	.anders fieldset {
		margin: 0.8rem 0 0.4rem;
		padding: 0.5rem 0.9rem 0.8rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
	}
	.merken {
		display: flex;
		flex-wrap: wrap;
		align-items: end;
		gap: 0.5rem;
	}
	.suche {
		display: flex;
		flex-wrap: wrap;
		align-items: end;
		gap: 0.5rem;
	}
	.treffer {
		display: grid;
		justify-items: start;
		gap: 0.15rem;
		margin: 0.3rem 0;
	}
	/* Die Statuszeile bleibt im Baum, damit Vorlese-Programme die Rückmeldung ansagen. */
	.leer {
		margin: 0;
	}
	.knoepfe {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}
	.zeilen {
		display: grid;
		gap: 0.35rem;
		margin: 0.4em 0 0.8em;
	}
	.zeilen > li {
		display: grid;
		grid-template-columns: minmax(8rem, 12rem) 1fr;
		gap: 0.2rem 0.75rem;
		padding: 0.35rem 0;
		border-bottom: 1px solid var(--line);
		overflow-wrap: anywhere;
		min-width: 0;
	}
	.zeilen > li:last-child {
		border-bottom: 0;
	}
	.was {
		display: grid;
		gap: 0.1rem;
		min-width: 0;
	}
	@media (max-width: 37.5rem) {
		.zeilen > li {
			grid-template-columns: 1fr;
		}
	}
</style>
