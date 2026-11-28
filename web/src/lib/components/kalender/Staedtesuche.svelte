<script lang="ts">
	/**
	 * Städtesuche (Kalender, aus /kalender herausgelöst in Stufe 7b, auch auf /heute):
	 * Suchfeld, Treffer als Knöpfe und die Namensnennung der Städteliste (Lizenzpflicht).
	 * Der Suchtext geht nur im Rumpf von `POST /api/kalender/staedte`, nie in die
	 * Adresse und nie in den Speicher des Browsers.
	 *
	 * Treffer mit Landnamen (`Intl.DisplayNames`, sonst der Ländercode), gleichnamige
	 * Orte eines Lands mit gerundeter Breite. Was nach der Wahl geschieht, sagt
	 * `onwahl`; gibt es ein Versprechen zurück, sind die Treffer bis dahin gesperrt.
	 * Fehler gehen als Satz an `onfehler`.
	 */
	import { Knopf } from '$katalog';
	import { API, fehlerText, senden } from '#lib/api/client.ts';
	import type { KalenderStadt, KalenderStaedte, KalenderStaedteAnfrage } from '$vertrag';

	interface Props {
		/** Nach dem Klick auf einen Treffer: die Stadt und ihr angezeigter Name. */
		onwahl: (s: KalenderStadt, text: string) => void | Promise<void>;
		onfehler: (satz: string) => void;
	}
	let { onwahl, onfehler }: Props = $props();

	let suchtext = $state('');
	let staedte = $state<KalenderStaedte | null>(null);
	let laeuft = $state(false);
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
			if (nr === suchNr) onfehler(fehlerText(e));
		}
	}

	/** Enter im Suchfeld sucht, statt ein umgebendes Formular abzuschicken. */
	function suchTaste(e: KeyboardEvent): void {
		if (e.key !== 'Enter') return;
		e.preventDefault();
		void stadtSuchen();
	}

	async function waehlen(s: KalenderStadt, alle: KalenderStadt[]): Promise<void> {
		const r = onwahl(s, stadtText(s, alle));
		if (r instanceof Promise) {
			laeuft = true;
			try {
				await r;
			} finally {
				laeuft = false;
			}
		}
		staedte = null;
	}

	const grad = (n: number): string => `${n.toFixed(1).replace('.', ',')}°`;

	/** Landname auf Deutsch; ohne `Intl.DisplayNames` oder bei unbekanntem Code der Code selbst. */
	const landNamen = (() => {
		try {
			return new Intl.DisplayNames(['de'], { type: 'region', fallback: 'none' });
		} catch {
			return null;
		}
	})();
	function landName(code: string): string {
		try {
			// `of` gibt ohne Namen undefined; ein leerer Name zählt ebenso als keiner.
			const name = landNamen?.of(code) ?? '';
			return name === '' ? code : name;
		} catch {
			return code;
		}
	}

	/**
	 * „Frankfurt (Deutschland)“; gibt es unter `alle` einen gleichnamigen Ort im selben Land,
	 * dazu die gerundete Breite, und lautet auch die gleich, die Länge. Verglichen wird der
	 * angezeigte Text, damit zwei Knöpfe fast nie gleich heißen. Ohne Land keine leeren Klammern.
	 */
	function stadtText(s: KalenderStadt, alle: KalenderStadt[]): string {
		const nachBreite = (t: KalenderStadt): string =>
			`${grad(Math.abs(t.breite))} ${t.breite < 0 ? 'S' : 'N'}`;
		const gleich = alle.filter(
			(t) => t !== s && t.name === s.name && t.land.trim() === s.land.trim()
		);
		const teile = [landName(s.land.trim())].filter((x) => x !== '');
		if (gleich.length > 0) teile.push(nachBreite(s));
		if (gleich.some((t) => nachBreite(t) === nachBreite(s)))
			teile.push(`${grad(Math.abs(s.laenge))} ${s.laenge < 0 ? 'W' : 'O'}`);
		return teile.length === 0 ? s.name : `${s.name} (${teile.join(', ')})`;
	}
</script>

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
							gesperrt={laeuft}
							onclick={() => {
								void waehlen(s, staedte?.treffer ?? []);
							}}>{stadtText(s, staedte.treffer)}</Knopf
						>
					</li>
				{/each}
			</ul>
		{/if}
	{/if}
</div>
<!-- Namensnennung der Städteliste (Lizenzpflicht), dauerhaft am Suchfeld. -->
<p class="muted small" data-quelle>
	Städteliste: {staedte?.quelle ?? 'GeoNames (geonames.org), CC BY 4.0'}
</p>

<style>
	.suche {
		display: flex;
		flex-wrap: wrap;
		align-items: end;
		gap: 0.5rem;
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
		border: 1px solid var(--linie);
		border-radius: 0.5rem;
		background: var(--hintergrund);
		color: var(--vordergrund);
	}
	.treffer {
		display: grid;
		justify-items: start;
		gap: 0.15rem;
		margin: 0.3rem 0;
	}
</style>
