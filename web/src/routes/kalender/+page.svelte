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
	 *
	 * Gebetszeiten (Stufe 4): Die Auswahl der Varianten speichert die Seite für
	 * benannte Orte (`POST /api/kalender/gebete`); beim anderen Ort geht sie nur
	 * im Rumpf der Abfrage mit und ist mit ihm vergessen.
	 *
	 * Zählungen (Stufe 5): die aktiven Zählkalender des Tags in der Reihenfolge der
	 * Vorlage, unabhängig vom Ort; eingeschaltet über `kalender = [...]` einer fristen.toml.
	 *
	 * Städtesuche (Baustein `kalender/Staedtesuche.svelte`): Eine gewählte Stadt füllt Breite,
	 * Länge und Zeitzone. Ihr Name steht nach dem Rechnen statt „Abgefragter Ort“ in der Anzeige;
	 * er geht nie an den Server und wird nicht gespeichert.
	 *
	 * Stufe 6: Der gewählte Ort wird auf Knopfdruck Heimatort (`POST /api/kalender/heimat`);
	 * „Das Jahr“ zeigt das Festjahr des Jahreskreises am Ort als Kreis und Liste.
	 *
	 * Stufe 7b: Startort ist der Blickort (Kopf aus dem gemeinsamen Laden `himmel`); der Tag und
	 * die Spanne der Feste beginnen beim Tag am Blickort, solange der Kopf fehlt beim Gerätetag.
	 */
	import { onMount, tick } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { resolve } from '$app/paths';
	import { Abschnitt, Absatz, Auswahl, Hinweis, Knopf, Schalter } from '$katalog';
	import { API, fehlerText, getJson, senden } from '#lib/api/client.ts';
	import { himmel } from '#lib/himmel.svelte.ts';
	import { fmtDate } from '#lib/format.ts';
	import { festjahrName } from '#lib/textfassung.ts';
	import { nameKurz } from '#lib/fristen.ts';
	import { lokalerTag } from '#lib/tageswechsel.ts';
	import { tagPlus } from '#lib/zeit/tage.ts';
	import { seitenTitel } from '#lib/titel.svelte.ts';
	import Jahreskreis from '#lib/components/kalender/Jahreskreis.svelte';
	import Staedtesuche from '#lib/components/kalender/Staedtesuche.svelte';
	import type {
		KalenderAbfrage,
		KalenderAbfrageAntwort,
		KalenderFeste,
		KalenderGebete,
		KalenderGebeteSpeichern,
		KalenderHeimat,
		KalenderJahr,
		KalenderMerken,
		KalenderOrt,
		KalenderOrte,
		KalenderStadt,
		KalenderTag,
		KalenderZonen,
		Zeitraum
	} from '$vertrag';

	/** Die Feste ab heute, so viele Tage weit. */
	const FESTE_TAGE = 30;

	const geraeteTag = lokalerTag(new Date());
	/** Der Tag am Blickort, sobald der Kopf da ist; vorher der des Geräts. */
	const heute = $derived(himmel.kopf?.datum ?? geraeteTag);
	const festeBis = $derived(tagPlus(heute, FESTE_TAGE));

	let orte = $state<KalenderOrt[] | null>(null);
	let ort = $state('');
	/** Hat jemand den Ort gewählt (Auswahl, Merken, Heimat), folgt er nicht mehr dem Blickort. */
	let ortGewaehlt = false;
	// Folgt `heute`, bis jemand einen anderen Tag einträgt.
	let datum = $derived(heute);
	let tag = $state<KalenderTag | null>(null);
	let feste = $state<KalenderFeste | null>(null);
	let jahr = $state<KalenderJahr | null>(null);
	let fehler = $state<string | null>(null);

	// „Anderer Ort“: Eingaben und der abgeschickte Ort, nur hier im Speicher.
	let breite = $state<number | null>(null);
	let laenge = $state<number | null>(null);
	let zeitzone = $state('');
	let frei = $state<{ breite: number; laenge: number; zeitzone: string | null } | null>(null);
	/** Die zuletzt gewählte Stadt (Anzeigename und Werte) und der Name zum abgeschickten Ort. */
	let stadt = $state<{ text: string; breite: number; laenge: number; zeitzone: string } | null>(
		null
	);
	let freiName = $state<string | null>(null);
	/** Was zum anderen Ort gerechnet wurde; `name` nur für die Anzeige, nie im Rumpf. */
	let gerechnet = $state<
		| (Pick<KalenderAbfrageAntwort, 'breite' | 'laenge' | 'naeherung'> & { name: string | null })
		| null
	>(null);

	// „Ort merken“ und „Als Heimatort“: Titel und Rückmeldung (ein ganzer Satz).
	let merkTitel = $state('');
	let gemerkt = $state<string | null>(null);
	let gemerktFeld = $state<HTMLElement | null>(null);
	let zonen = $state<KalenderZonen | null>(null);

	// Gebetszeiten: Varianten und gespeicherte Auswahl; beim anderen Ort nur hier im Speicher.
	let gebete = $state<KalenderGebete | null>(null);
	let freiGebete = $state<string[]>([]);
	/**
	 * Die gewünschten, noch nicht bestätigten Stände je Ort: Klicks während des Speicherns
	 * gehen nicht verloren, auch nicht bei einem schnellen Ortswechsel.
	 */
	const wuensche = new SvelteMap<string, string[]>();
	let speichertGebete = false;
	const gewaehlt = $derived(
		frei !== null
			? freiGebete
			: (wuensche.get(ort) ?? gebete?.auswahl.find((a) => a.ort === ort)?.varianten ?? [])
	);

	let breiteFeld = $state<HTMLInputElement | null>(null);

	/** Übernimmt die Stadt in Breite, Länge und Zeitzone; rechnen erst auf Knopfdruck. */
	function stadtWaehlen(s: KalenderStadt, text: string): void {
		stadt = { text, breite: s.breite, laenge: s.laenge, zeitzone: s.zeitzone };
		breite = s.breite;
		laenge = s.laenge;
		zeitzone = s.zeitzone;
		breiteFeld?.focus();
	}

	const optionen = $derived(
		(orte ?? []).map((o) => ({ wert: o.kennung, text: o.heimat ? `${o.titel} (Heimat)` : o.titel }))
	);

	/** Lädt die Orte, der Heimatort zuerst; gewählt wird `waehlen`, sonst der Blickort oder die Heimat. */
	async function orteLaden(waehlen: string | null): Promise<void> {
		const antwort = await getJson<KalenderOrte>(`${API}/kalender/standorte`);
		if (waehlen !== null) ortGewaehlt = true;
		const da = (k: string | undefined): k is string =>
			k !== undefined && antwort.orte.some((o) => o.kennung === k);
		const blick = himmel.kopf?.kennung;
		// Der Server liefert den Heimatort zuerst.
		ort = waehlen ?? (!ortGewaehlt && da(blick) ? blick : (antwort.orte[0]?.kennung ?? ''));
		orte = antwort.orte;
	}

	// Kommt der Kopf erst nach den Orten, wechselt der Startort noch zum Blickort.
	$effect(() => {
		const blick = himmel.kopf?.kennung;
		if (blick === undefined || ortGewaehlt || frei !== null || orte === null) return;
		if (orte.some((o) => o.kennung === blick)) ort = blick;
	});

	onMount(() => {
		const ab = himmel.nutzen();
		orteLaden(null).catch((e: unknown) => {
			fehler = fehlerText(e);
		});
		// Ohne Zonenliste bleibt das Feld frei beschreibbar; kein Fehler auf der Seite.
		getJson<KalenderZonen>(`${API}/kalender/zonen`).then(
			(z) => (zonen = z),
			() => (zonen = null)
		);
		getJson<KalenderGebete>(`${API}/kalender/gebete`).then(
			(g) => (gebete = g),
			() => (gebete = null)
		);
		return ab;
	});

	/** Nur die letzte Anfrage zählt; eine langsamere ältere überschreibt nichts. */
	let laufend = 0;

	async function laden(
		d: string,
		o: string,
		f: { breite: number; laenge: number; zeitzone: string | null } | null,
		g: string[] = []
	): Promise<void> {
		const nr = ++laufend;
		try {
			if (f === null) {
				const ortTeil = o === '' ? '' : `&ort=${encodeURIComponent(o)}`;
				const [t, fe, j] = await Promise.all([
					getJson<KalenderTag>(`${API}/kalender/tag?datum=${encodeURIComponent(d)}${ortTeil}`),
					getJson<KalenderFeste>(`${API}/kalender/feste?von=${heute}&bis=${festeBis}${ortTeil}`),
					getJson<KalenderJahr>(`${API}/kalender/jahr?datum=${encodeURIComponent(d)}${ortTeil}`)
				]);
				if (nr !== laufend) return;
				tag = t;
				feste = fe;
				jahr = j;
				gerechnet = null;
			} else {
				const name = freiName;
				const koerper: KalenderAbfrage = {
					datum: d,
					von: heute,
					bis: festeBis,
					breite: f.breite,
					laenge: f.laenge,
					zeitzone: f.zeitzone,
					gebete: g.length > 0 ? g : null,
					jahr: true
				};
				const a = await senden<KalenderAbfrageAntwort>(`${API}/kalender/abfrage`, koerper);
				if (nr !== laufend) return;
				tag = a.tag;
				feste = a.feste;
				jahr = a.jahr;
				gerechnet = { breite: a.breite, laenge: a.laenge, naeherung: a.naeherung, name };
			}
			fehler = null;
		} catch (e) {
			if (nr === laufend) fehler = fehlerText(e);
		}
	}

	$effect(() => {
		if (orte === null || datum === '') return;
		void laden(datum, ort, frei, [...freiGebete]);
	});

	// „Das Jahr“: welches Festjahr der Kreis zeigt, gezählt ab dem mit dem gewählten Tag.
	let versatz = $state('0');
	let anderesJahr = $state<{ versatz: string; jahr: KalenderJahr } | null>(null);
	let jahrNr = 0;
	const gezeigt = $derived(
		versatz !== '0' && anderesJahr?.versatz === versatz ? anderesJahr.jahr : jahr
	);
	/** „2025/26“ um `v` Jahre versetzt; jede Zahl behält ihre Stellen („2099/00“). */
	function versetzt(name: string, v: number): string {
		return name.replace(/\d+/g, (z) => {
			const n = 10 ** z.length;
			return String((((Number(z) + v) % n) + n) % n).padStart(z.length, '0');
		});
	}
	const jahrOptionen = $derived.by(() => {
		if (jahr === null) return [];
		const name = festjahrName(jahr);
		return [-1, 0, 1].map((v) => ({
			wert: String(v),
			text: `${versetzt(name, v)}${v === 0 ? ' (mit dem gewählten Tag)' : ''}`
		}));
	});

	$effect(() => {
		const basis = jahr;
		const v = Number(versatz);
		const [o, f] = [ort, frei];
		// Auch bei v = 0 hochzählen: eine noch laufende Antwort für ±1 zählt dann nicht mehr.
		const nr = ++jahrNr;
		if (basis === null || v === 0) return;
		// Die Mitte des Festjahrs um `v` Jahre weiter liegt sicher im gesuchten: Festjahre
		// beginnen höchstens einen Monat früher oder später und dauern mindestens 354 Tage.
		const d = tagPlus(basis.zeitraum.anfang, 182 + 365 * v);
		void (async () => {
			try {
				const j =
					f === null
						? await getJson<KalenderJahr>(
								`${API}/kalender/jahr?datum=${d}${o === '' ? '' : `&ort=${encodeURIComponent(o)}`}`
							)
						: (
								await senden<KalenderAbfrageAntwort>(`${API}/kalender/abfrage`, {
									datum: d,
									von: heute,
									bis: festeBis,
									breite: f.breite,
									laenge: f.laenge,
									zeitzone: f.zeitzone,
									gebete: null,
									jahr: true
								} satisfies KalenderAbfrage)
							).jahr;
				// Die nächsten Vollmonde zählen ab dem gewählten Tag, nicht ab der Mitte des anderen Jahrs.
				if (nr !== jahrNr) return;
				if (j === null) throw new Error('Für dieses Festjahr lässt sich nichts rechnen.');
				anderesJahr = {
					versatz: String(v),
					jahr: { ...j, naechste_vollmonde: basis.naechste_vollmonde }
				};
				fehler = null;
			} catch (e) {
				// Auswahl und Kreis sollen dasselbe Jahr nennen: zurück zum Jahr mit dem Tag.
				if (nr !== jahrNr) return;
				fehler = fehlerText(e);
				versatz = '0';
			}
		})();
	});

	/** Ein Schalter einer Variante: beim anderen Ort nur für die Abfrage, sonst gespeichert. */
	async function umschalten(kennung: string, an: boolean): Promise<void> {
		const neu = an ? [...gewaehlt, kennung] : gewaehlt.filter((k) => k !== kennung);
		if (frei !== null) {
			freiGebete = neu;
			return;
		}
		wuensche.set(ort, neu);
		// Läuft schon ein Speichern, schickt die Schleife danach die neuesten Wünsche jedes Orts.
		if (speichertGebete) return;
		speichertGebete = true;
		try {
			for (let e = wuensche.entries().next(); !e.done; e = wuensche.entries().next()) {
				const [o, w] = e.value;
				// Schlägt es fehl, zeigen die Schalter dieses Orts wieder den gespeicherten Stand.
				const gut = await gebeteSpeichern(o, w);
				if (!gut || wuensche.get(o) === w) wuensche.delete(o);
			}
		} finally {
			speichertGebete = false;
		}
	}

	/** Speichert die Auswahl eines Orts; `false`, wenn es nicht ging (der Satz steht dann auf der Seite). */
	async function gebeteSpeichern(o: string, varianten: string[]): Promise<boolean> {
		const koerper: KalenderGebeteSpeichern = { ort: o, varianten };
		try {
			gebete = await senden<KalenderGebete>(`${API}/kalender/gebete`, koerper);
			fehler = null;
			// Mit dem Stand von jetzt: Ist inzwischen ein anderer Ort abgefragt, bleibt er.
			await laden(datum, ort, frei, [...freiGebete]);
			return true;
		} catch (e) {
			fehler = fehlerText(e);
			return false;
		}
	}

	function abschicken(e: SubmitEvent): void {
		e.preventDefault();
		if (breite === null || laenge === null) return;
		const zone = zeitzone.trim();
		frei = { breite, laenge, zeitzone: zone === '' ? null : zone };
		// Der Name gilt nur, solange die Felder noch genau die gewählte Stadt zeigen.
		freiName =
			stadt !== null &&
			stadt.breite === breite &&
			stadt.laenge === laenge &&
			stadt.zeitzone === zone
				? stadt.text
				: null;
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
			gemerkt = `Gemerkt als „${neu.titel}“.`;
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
		freiName = null;
		stadt = null;
		breite = null;
		laenge = null;
		zeitzone = '';
		merkTitel = '';
		freiGebete = [];
	}

	/**
	 * Macht den gewählten Ort zum Heimatort; danach die Orte neu, der Ort bleibt gewählt.
	 * Der Knopf ist dann weg; der Fokus geht wie bei „Ort merken“ auf die Rückmeldung.
	 */
	async function alsHeimat(): Promise<void> {
		const koerper: KalenderHeimat = { ort };
		// Vor dem Warten festhalten: Wird währenddessen ein anderer Ort gewählt, nennt die Rückmeldung trotzdem diesen.
		const titel = gewaehltOrt?.titel ?? ort;
		try {
			await senden<KalenderOrte>(`${API}/kalender/heimat`, koerper);
			await orteLaden(ort);
			gemerkt = `Heimatort ist jetzt „${titel}“.`;
			fehler = null;
			await tick();
			gemerktFeld?.focus();
		} catch (e) {
			fehler = fehlerText(e);
		}
	}

	const gewaehltOrt = $derived(orte?.find((o) => o.kennung === ort) ?? null);

	const grad = (n: number): string => `${n.toFixed(1).replace('.', ',')}°`;

	/**
	 * Herkunft für die Anzeige: Der Server nennt den anderen Ort „ · <tag.ort> · “; ist der Name
	 * der gewählten Stadt bekannt, steht er dort wie in der Kopfzeile. Nur Anzeige.
	 */
	function herkunftZeigen(herkunft: string): string {
		const name = gerechnet?.name ?? null;
		const platzhalter = tag?.ort ?? null;
		if (name === null || platzhalter === null) return herkunft;
		// Als Funktion, damit ein „$“ im Namen kein Ersetzungsmuster wird.
		return herkunft.replaceAll(` · ${platzhalter} · `, () => ` · ${name} · `);
	}
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
		<Auswahl
			beschriftung="Ort"
			{optionen}
			bind:wert={ort}
			onwechsel={() => {
				ortGewaehlt = true;
				vergessen();
			}}
		/>
		{#if gewaehltOrt !== null && !gewaehltOrt.heimat && frei === null}
			<Knopf onclick={() => void alsHeimat()}>Als Heimatort</Knopf>
		{/if}
	{/if}
	<label class="feld">
		Tag
		<input type="date" bind:value={datum} required />
	</label>

	<form class="anders" onsubmit={abschicken} autocomplete="off">
		<fieldset>
			<legend>Anderer Ort</legend>
			<Staedtesuche onwahl={stadtWaehlen} onfehler={(satz: string) => (fehler = satz)} />
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
		{gemerkt ?? ''}
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
			<!-- Das Leerzeichen ausdrücklich: Svelte kürzt Leerraum am Ende eines Blocks. -->
			{#if tag.ort}{`${gerechnet?.name ?? tag.ort} · `}{/if}Uhrzeiten in {tag.zeitart}
		</p>
		{#if tag.zeiten.length > 0}
			<ul class="plain zeilen">
				{#each tag.zeiten as z (z.kennung)}
					<li>
						<span class="wann"><b>{z.zeit ?? z.text ?? ''}</b></span>
						<span class="was">
							{z.name}
							<span class="muted small">{herkunftZeigen(z.herkunft)}</span>
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

	{#if gebete !== null && gebete.varianten.length > 0}
		<Abschnitt titel="Gebetszeiten">
			{#if gebete.hinweis}
				<Hinweis>{gebete.hinweis}</Hinweis>
			{/if}
			{#each gebete.auswahl.filter((a) => a.fehlt) as a (a.ort)}
				<p data-fehlt>
					Den Ort „{a.ort}“ gibt es nicht mehr.
					<Knopf art="leise" onclick={() => void gebeteSpeichern(a.ort, [])}
						>Seine Auswahl entfernen</Knopf
					>
				</p>
			{/each}
			<fieldset class="varianten">
				<legend>Varianten für {frei === null ? ortTitel : 'den anderen Ort'}</legend>
				{#each gebete.varianten as v (v.kennung)}
					<Schalter
						an={gewaehlt.includes(v.kennung)}
						beschriftung={v.titel}
						onwechsel={(an: boolean) => void umschalten(v.kennung, an)}
					/>
				{/each}
			</fieldset>
			<p class="muted small">
				{#if frei === null}Die Auswahl wird für diesen Ort gespeichert.{:else}Für den anderen Ort
					gilt die Auswahl nur, solange du auf dieser Seite bist.{/if}
			</p>
			{#if tag.gebete.length > 0}
				<div class="gebete">
					{#each tag.gebete as l (l.kennung)}
						<section class="gebetsliste" aria-label={l.titel}>
							<h3>{l.titel}</h3>
							{#if l.hinweis}
								<Hinweis>{l.hinweis}</Hinweis>
							{/if}
							<ul class="plain zeilen">
								{#each l.zeilen as z, i (i)}
									<li>
										<span class="wann"><b>{z.zeit ?? z.text ?? ''}</b></span>
										<span class="was">
											{z.name}
											{#if z.umfasst.length > 0}<span class="small">mit {z.umfasst.join(', ')}</span
												>{/if}
											{#if z.hinweis}<span class="small" data-hochbreite>{z.hinweis}</span>{/if}
											<span class="muted small">{herkunftZeigen(z.herkunft)}</span>
										</span>
									</li>
								{/each}
							</ul>
						</section>
					{/each}
				</div>
			{/if}
		</Abschnitt>
	{/if}

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

	{#if gezeigt !== null}
		<Abschnitt
			titel="Das Jahr"
			unterzeile="Festjahr des Jahreskreises vom {fmtDate(
				gezeigt.zeitraum.anfang
			)} bis vor den {fmtDate(gezeigt.zeitraum.ende)}"
		>
			<div class="jahrwahl">
				<Auswahl beschriftung="Festjahr" optionen={jahrOptionen} bind:wert={versatz} />
			</div>
			<Jahreskreis jahr={gezeigt} tag={tag.datum} />
		</Abschnitt>
	{/if}
{/if}

{#if tag !== null && tag.zaehlungen.length > 0}
	<Abschnitt titel="Zählungen">
		<ul class="plain zeilen">
			{#each tag.zaehlungen as z (z.kennung)}
				<li data-zaehlung={z.kennung}>
					<span class="wann"><b>{z.titel}</b></span>
					<span class="was">
						<span
							>{z.stellung}{#if z.stufe !== 'belegt'}
								<span class="muted small">({z.stufe})</span>{/if}</span
						>
						{#if z.hinweis}<span class="small">{z.hinweis}</span>{/if}
						<span class="muted small">{herkunftZeigen(z.herkunft)}</span>
					</span>
				</li>
			{/each}
		</ul>
	</Abschnitt>
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
							<!-- Die Herkunft beginnt schon mit dem Namen des Kalenders. -->
							<span class="muted small">{herkunftZeigen(f.herkunft)}</span>
						</span>
					</li>
				{/each}
			</ul>
		{/if}
	</Abschnitt>
{/if}

<style>
	.jahrwahl {
		margin-bottom: 0.75rem;
	}
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
	.varianten {
		display: grid;
		gap: 0.4rem;
		margin: 0.4rem 0;
		padding: 0.5rem 0.9rem 0.8rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
	}
	/* Die Varianten nebeneinander, schmal untereinander. */
	.gebete {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
		gap: 0.5rem 1.25rem;
	}
	.gebetsliste h3 {
		margin: 0.6em 0 0.2em;
		font-size: var(--text-md);
	}
	.gebetsliste .zeilen > li {
		grid-template-columns: minmax(5rem, 8rem) 1fr;
	}
	@media (max-width: 37.5rem) {
		.zeilen > li {
			grid-template-columns: 1fr;
		}
	}
</style>
