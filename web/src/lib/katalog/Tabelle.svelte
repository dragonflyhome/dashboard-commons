<script lang="ts">
	/**
	 * Tabelle (Etappe 3a-3): eine Tabelle, die nie die Seite verbreitert. Ist
	 * sie breiter als der Platz, rollt sie in ihrem eigenen Rahmen seitlich;
	 * Schatten am Rand zeigen, dass dort mehr kommt (scrollShadow). Nur dann
	 * ist der Rahmen ein benannter Bereich (role=region mit dem Namen der
	 * Tabelle) mit Tab-Stopp (Tab, dann Pfeile), damit Vorlese-Programme
	 * wissen, was dort rollt; passt die Tabelle, gibt es keinen leeren
	 * Tab-Stopp. Gemessen wird mit einem ResizeObserver (scrollWidth größer
	 * als clientWidth).
	 *
	 * Drei Formen:
	 *   Mit Daten:   <Tabelle beschriftung="Wo was liegt" spalten={…} zeilen={…} />
	 *   Aus einer Datei (Etappe B1), Zeilen sind die Einträge unter `liste`:
	 *     <Tabelle beschriftung="Kosten" daten={ladeDaten('eigen-kosten')} liste="posten" spalten={…} />
	 *   Eigene Zeilen (etwa mit Links in den Zellen):
	 *     <Tabelle beschriftung="Wo was liegt">
	 *       <thead><tr><th>Thema</th>…</tr></thead>
	 *       <tbody>{#each …}<tr><td>…</td></tr>{/each}</tbody>
	 *     </Tabelle>
	 * `beschriftung` wird die Überschrift der Tabelle (caption); mit
	 * `beschriftungZeigen={false}` nur für Vorlese-Programme, wenn darüber
	 * schon eine Überschrift steht.
	 *
	 * Etappe B1, je Spalte (grund/typen.ts):
	 *   - `summe: true`: Summenzeile unten; die Summe rechnet der Server
	 *     (`abgeleitet.listen.<liste>.summe`), die Tabelle rechnet nie selbst;
	 *   - `link: 'adresse'`: die Zelle wird ein Link mit der Adresse aus dieser
	 *     Spalte der Zeile (https in neuem Tab oder ein Pfad dieser App);
	 *   - `art: 'marke'`: das Wort als <Marke>, die Bedeutung je Zeile vom
	 *     Server (`abgeleitet.listen.<liste>.marken`), sonst neutral.
	 * Aus einer Datei nimmt die Tabelle das Abgeleitete selbst; wer `zeilen`
	 * selbst baut, gibt es mit `abgeleitet` dazu.
	 */
	import type { Snippet } from 'svelte';
	import type { Bedeutung, ListeAbgeleitet } from '$vertrag';
	import { sichererLink } from '#lib/aufgaben.ts';
	import { istZahl } from '#lib/format.ts';
	import { scrollShadow } from '#lib/scrollschatten.ts';
	import type { Spalte, Zeile } from './grund/typen.ts';
	import Marke from './Marke.svelte';

	interface Props {
		beschriftung: string;
		beschriftungZeigen?: boolean;
		spalten?: readonly Spalte[];
		zeilen?: readonly Zeile[];
		/**
		 * Kleinste Breite in rem, unter der die Tabelle seitlich rollt statt
		 * die Spalten weiter zu quetschen (etwa 32 für vier Textspalten).
		 * 0: so schmal, wie der Inhalt erlaubt.
		 */
		mindestbreite?: number;
		/** Satz, wenn `zeilen` leer ist. */
		leer?: string;
		/** Eigene `<thead>` und `<tbody>` statt `spalten` und `zeilen`. */
		children?: Snippet;
		/**
		 * Etappe B1: der Inhalt einer Datei aus `daten/` oder gleich das Objekt
		 * aus `ladeDaten`; die Zeilen sind dann die Einträge unter `liste`.
		 */
		daten?: unknown;
		/** Pfad der Liste in `daten`, etwa `posten` oder `teil.plan.posten`. */
		liste?: string | undefined;
		/** Was der Server aus dieser Liste rechnet (Summen, Marken); sonst aus `daten`. */
		abgeleitet?: ListeAbgeleitet | null | undefined;
		/** Wort am Anfang der Summenzeile. */
		summeWort?: string;
	}
	let {
		beschriftung,
		beschriftungZeigen = false,
		spalten = [],
		zeilen = [],
		mindestbreite = 0,
		leer = 'Noch keine Einträge.',
		children,
		daten,
		liste,
		abgeleitet,
		summeWort = 'Summe'
	}: Props = $props();
	const id = $props.id();

	let rahmen: HTMLDivElement | undefined = $state();
	let rollt = $state(false);
	$effect(() => {
		const el = rahmen;
		if (!el) return;
		const messen = (): void => {
			rollt = el.scrollWidth > el.clientWidth;
		};
		messen();
		if (typeof ResizeObserver === 'undefined') return;
		// Rahmen (Fensterbreite) und Tabelle (Inhalt) ändern den Überstand.
		const ro = new ResizeObserver(messen);
		ro.observe(el);
		for (const kind of el.children) ro.observe(kind);
		return () => {
			ro.disconnect();
		};
	});

	// ----- Tabelle aus einer Datei (Etappe B1) -----

	/** Das, was `ladeDaten` liefert (nur die Felder, die die Tabelle braucht). */
	interface Geladen {
		zustand: 'laedt' | 'fertig' | 'fehler';
		inhalt: unknown;
		abgeleitet: { listen: Partial<Record<string, ListeAbgeleitet>> } | null;
		satz: string | null;
	}
	function istGeladen(d: unknown): d is Geladen {
		return typeof d === 'object' && d !== null && 'zustand' in d && 'neuLaden' in d;
	}
	function istObjekt(v: unknown): v is Record<string, unknown> {
		return typeof v === 'object' && v !== null && !Array.isArray(v);
	}

	/**
	 * Die Einträge unter `pfad`, wie der Server sie zählt: verschachtelte
	 * Listen über alle Eltern, in Reihenfolge der Datei. Fehlt der Schlüssel
	 * oder ist das Array leer, ist die Liste leer; steht dort etwas anderes
	 * als eine Liste von Einträgen: `null`.
	 */
	function eintraege(inhalt: unknown, pfad: string): Zeile[] | null {
		let stufe: unknown[] = [inhalt];
		for (const teil of pfad.split('.')) {
			const naechste: unknown[] = [];
			for (const knoten of stufe) {
				for (const k of Array.isArray(knoten) ? knoten : [knoten]) {
					if (istObjekt(k) && teil in k) naechste.push(k[teil]);
				}
			}
			stufe = naechste;
		}
		const aus: Zeile[] = [];
		for (const knoten of stufe) {
			if (!Array.isArray(knoten)) return null;
			for (const e of knoten) {
				if (!istObjekt(e)) return null;
				aus.push(e as Zeile);
			}
		}
		return aus;
	}

	const geladen = $derived(istGeladen(daten) ? daten : null);
	const ausDatei = $derived(daten !== undefined && liste !== undefined);
	const inhalt = $derived(geladen === null ? (daten ?? null) : geladen.inhalt);
	const dateiZeilen = $derived(ausDatei && inhalt !== null ? eintraege(inhalt, liste ?? '') : null);
	const alleZeilen = $derived(ausDatei ? (dateiZeilen ?? []) : zeilen);
	const abg = $derived(
		abgeleitet ?? (liste === undefined ? null : (geladen?.abgeleitet?.listen[liste] ?? null))
	);
	const mitSumme = $derived(spalten.some((s) => s.summe === true));

	function zelle(z: Zeile, s: Spalte): string {
		const w = z[s.schluessel];
		if (w === null || w === undefined) return '';
		if (typeof w === 'boolean') return w ? 'ja' : 'nein';
		return String(w);
	}

	/** Bedeutung der Marke in Zeile `i`, wie der Server das Wort gelesen hat; sonst neutral. */
	function marke(s: Spalte, i: number): Bedeutung {
		return abg?.marken[s.schluessel]?.[i] ?? 'neutral';
	}

	/** Pfad dieser App: ein `/` vorn, kein zweiter, kein `\`, keine Leerzeichen. */
	const APP_PFAD = /^\/(?![/\\])[^\s\\]*$/;

	/** Link einer Zelle: https (neuer Tab) oder ein Pfad dieser App; sonst keiner. */
	function linkVon(z: Zeile, s: Spalte): { href: string; extern: boolean } | null {
		if (s.link === undefined) return null;
		const w = z[s.link];
		if (typeof w !== 'string') return null;
		const a = w.trim();
		if (APP_PFAD.test(a)) return { href: a, extern: false };
		const sicher = sichererLink(a);
		return sicher === null ? null : { href: sicher, extern: true };
	}

	/** Summe einer Spalte vom Server als deutsche Zahl; ohne Summe des Servers leer. */
	function summe(s: Spalte): string {
		const n = abg?.summe[s.schluessel];
		// Abnahme B1: nie `null.toLocaleString` (aus nan oder inf wird im JSON null).
		return istZahl(n) ? n.toLocaleString('de-DE', { maximumFractionDigits: 2 }) : '';
	}
</script>

{#if ausDatei && geladen?.zustand === 'fehler'}
	<p class="block-error" role="alert">{geladen.satz ?? 'Die Daten lassen sich nicht laden.'}</p>
{:else if ausDatei && inhalt === null}
	<p class="laedt muted small" aria-busy="true">Daten werden geladen …</p>
{:else if ausDatei && dateiZeilen === null}
	<p class="block-error">In den Daten steht unter „{liste}“ keine Liste von Einträgen.</p>
{:else if !children && alleZeilen.length === 0}
	<p class="muted leer">{leer}</p>
{:else}
	<!-- svelte-ignore a11y_no_noninteractive_tabindex (rollt der Rahmen, ist er nach WCAG 2.1.1 per Tab erreichbar und nach der Tabelle benannt) -->
	<div
		bind:this={rahmen}
		class="scroll-x tabelle"
		role={rollt ? 'region' : undefined}
		aria-labelledby={rollt ? `${id}-name` : undefined}
		tabindex={rollt ? 0 : undefined}
		use:scrollShadow
	>
		<table style:--tabelle-min="{mindestbreite}rem">
			<caption id="{id}-name" class:nur-vorlesen={!beschriftungZeigen}>{beschriftung}</caption>
			{#if children}
				{@render children()}
			{:else}
				<thead>
					<tr>
						{#each spalten as s (s.schluessel)}
							<th scope="col" class:zahl={s.art === 'zahl'}>{s.titel}</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each alleZeilen as z, i (i)}
						<tr>
							{#each spalten as s (s.schluessel)}
								{@const text = zelle(z, s)}
								{@const ziel = linkVon(z, s)}
								<td class:mono={s.art === 'mono'} class:zahl={s.art === 'zahl'}>
									{#if s.art === 'marke' && text !== ''}
										<Marke bedeutung={marke(s, i)} wort={text} />
									{:else if ziel !== null && text !== ''}
										<!-- eslint-disable svelte/no-navigation-without-resolve -- Adresse aus den Daten: https (neuer Tab) oder ein fertiger Pfad dieser App -->
										<a
											href={ziel.href}
											target={ziel.extern ? '_blank' : undefined}
											rel={ziel.extern ? 'noopener noreferrer' : undefined}
											data-sveltekit-reload={ziel.extern ? '' : undefined}
											>{text}{#if ziel.extern}<span class="nur-vorlesen">
													(öffnet in neuem Tab)</span
												>{/if}</a
										>
										<!-- eslint-enable svelte/no-navigation-without-resolve -->
									{:else}
										{text}
									{/if}
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
				{#if mitSumme}
					<tfoot>
						<tr class="summe">
							{#each spalten as s, j (s.schluessel)}
								{#if s.summe === true}
									<td class:zahl={s.art === 'zahl'}>{summe(s)}</td>
								{:else if j === 0}
									<th scope="row">{summeWort}</th>
								{:else}
									<td></td>
								{/if}
							{/each}
						</tr>
					</tfoot>
				{/if}
			{/if}
		</table>
	</div>
{/if}

<style>
	.tabelle {
		border: 1px solid var(--line);
		border-radius: 10px;
	}
	.tabelle:focus-visible {
		outline-offset: 0;
	}
	table {
		width: 100%;
		min-width: var(--tabelle-min, 0);
		margin: 0;
	}
	caption {
		text-align: left;
		font-weight: 600;
		padding: 0.5rem 0.6rem 0;
	}
	.mono {
		font-family: var(--mono);
		white-space: nowrap;
	}
	.zahl {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.leer {
		margin: 0.4em 0 1em;
	}
	.laedt {
		margin: 0.6em 0 1em;
	}
	.summe {
		font-weight: 600;
		border-top: 2px solid var(--line);
	}
	.summe th {
		text-align: left;
	}
</style>
