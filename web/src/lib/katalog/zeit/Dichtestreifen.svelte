<script lang="ts">
	/**
	 * Schmaler Streifen über dem Verlauf (Zeitachse Fassung 1): je Tag der
	 * letzten 91 Tage (heute und die 90 davor, wie der Server liefert) ein
	 * Balken mit der Zahl der Einträge, darunter die gemeinsame Zeitachse mit
	 * der Heute-Linie. Antippen springt zum Tag, Ziehen wählt einen Zeitraum.
	 * Tastatur: Pfeile gehen von Tag zu Tag, Bild auf und Bild ab eine Woche,
	 * Enter springt hin, + und - zoomen (alle Tage, 30, 14), Pos1 geht auf
	 * heute, Ende auf den ältesten Tag, Esc hebt den Zeitraum auf. Alt+Pfeil
	 * bleibt beim Browser.
	 *
	 * Mit `aus` (die im Verlauf ausgeblendeten Quellen) zählt der Streifen nur,
	 * was der Verlauf auch zeigt. Ohne `jetzt` läuft Heute über Mitternacht
	 * weiter (`LaufendesJetzt`).
	 */
	import type { VerlaufEreignis, VerlaufQuelle } from '$vertrag';
	import Zeitachse from './Zeitachse.svelte';
	import {
		DICHTE_TAGE,
		ausschnittText,
		fensterMitCursor,
		fensterZoomen,
		laenge,
		naechsteSpanne,
		tageZaehlen,
		type TagFenster
	} from '#lib/zeit/dichte.ts';
	import { filtern, type Zeitraum } from '#lib/zeit/verlauf.ts';
	import { LaufendesJetzt } from '#lib/zeit/jetzt.svelte.ts';
	import { kurzDatum, tagPlus, tagText, tagTitel, tagZahl } from '#lib/zeit/tage.ts';

	interface Props {
		ereignisse: readonly VerlaufEreignis[];
		/** Ausgeblendete Quellen (derselbe Filter wie im Verlauf). */
		aus?: ReadonlySet<VerlaufQuelle>;
		/** Für Tests; sonst jetzt, über Mitternacht weiter. */
		jetzt?: Date;
		/** Wie viele Tage es insgesamt gibt. */
		tage?: number;
		/** Ein Tag wurde gewählt (Antippen, Enter). */
		onwaehle?: (tag: string) => void;
		/** Ein Zeitraum wurde gezogen, oder `null`: aufgehoben. */
		onzeitraum?: (z: Zeitraum | null) => void;
		/** Der gewählte Zeitraum (vom Verlauf aufgehoben wird er hier leer). */
		zeitraum?: Zeitraum | null;
	}
	let {
		ereignisse,
		aus = new Set<VerlaufQuelle>(),
		jetzt,
		tage = DICHTE_TAGE,
		onwaehle,
		onzeitraum,
		zeitraum = null
	}: Props = $props();

	const uhr = new LaufendesJetzt();
	const heute = $derived(tagText(jetzt ?? uhr.wert));
	const grenzen = $derived<TagFenster>({ von: tagPlus(heute, -(tage - 1)), bis: heute });
	const balken = $derived(tageZaehlen(filtern(ereignisse, aus), heute, tage));
	const hoechstes = $derived(Math.max(1, ...balken.map((b) => b.anzahl)));

	let cursor = $state<string | null>(null);
	let fenster = $state<TagFenster | null>(null);
	const c = $derived(cursor ?? heute);
	const f = $derived(fenster ?? grenzen);
	const sichtbar = $derived(balken.filter((b) => b.tag >= f.von && b.tag <= f.bis));
	const cursorBalken = $derived(balken.find((b) => b.tag === c));

	let breite = $state(0);
	const achsenFenster = $derived({ von: tagZahl(f.von), bis: tagZahl(f.bis) + 1 });

	function setzeCursor(tag: string): void {
		const t = tag < grenzen.von ? grenzen.von : tag > grenzen.bis ? grenzen.bis : tag;
		cursor = t;
		fenster = fensterMitCursor(f, t, grenzen);
	}
	function zoom(richtung: 1 | -1): void {
		fenster = fensterZoomen(naechsteSpanne(laenge(f), richtung, tage), c, grenzen);
	}
	function waehle(tag: string): void {
		setzeCursor(tag);
		onwaehle?.(tag);
	}

	function onkeydown(e: KeyboardEvent): void {
		if (e.altKey || e.ctrlKey || e.metaKey) return;
		switch (e.key) {
			case 'ArrowLeft':
				setzeCursor(tagPlus(c, -1));
				break;
			case 'ArrowRight':
				setzeCursor(tagPlus(c, 1));
				break;
			case '+':
			case '=':
				zoom(1);
				break;
			case '-':
				zoom(-1);
				break;
			case 'PageUp':
				setzeCursor(tagPlus(c, 7));
				break;
			case 'PageDown':
				setzeCursor(tagPlus(c, -7));
				break;
			case 'Home':
				setzeCursor(heute);
				break;
			case 'End':
				setzeCursor(grenzen.von);
				break;
			case 'Enter':
			case ' ':
				onwaehle?.(c);
				break;
			case 'Escape':
				if (zeitraum === null) return;
				onzeitraum?.(null);
				break;
			default:
				return;
		}
		e.preventDefault();
	}

	// Zeigen: Antippen oder Ziehen über die Balken.
	let streifen = $state<HTMLDivElement | null>(null);
	let zug = $state<{ start: string; x: number; gezogen: boolean; ende: string } | null>(null);
	function tagBei(clientX: number): string | null {
		if (streifen === null) return null;
		const r = streifen.getBoundingClientRect();
		if (r.width <= 0) return null;
		const i = Math.floor(((clientX - r.left) / r.width) * sichtbar.length);
		const b = sichtbar[Math.min(Math.max(i, 0), sichtbar.length - 1)];
		return b?.tag ?? null;
	}
	function onpointerdown(e: PointerEvent): void {
		if (e.button !== 0) return;
		const t = tagBei(e.clientX);
		if (t === null) return;
		zug = { start: t, x: e.clientX, gezogen: false, ende: t };
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
	}
	function onpointermove(e: PointerEvent): void {
		if (zug === null) return;
		const t = tagBei(e.clientX);
		if (t === null) return;
		const weit = Math.abs(e.clientX - zug.x) > 6;
		zug = { ...zug, ende: t, gezogen: zug.gezogen || (weit && t !== zug.start) };
	}
	function onpointerup(): void {
		if (zug === null) return;
		const { start, ende, gezogen } = zug;
		zug = null;
		if (gezogen) {
			const von = start < ende ? start : ende;
			const bis = start < ende ? ende : start;
			setzeCursor(bis);
			onzeitraum?.({ von, bis });
		} else {
			waehle(start);
		}
	}
	function onpointercancel(): void {
		zug = null;
	}

	// Während des Ziehens die Auswahl zeigen, sonst den gewählten Zeitraum.
	const auswahl = $derived.by((): Zeitraum | null => {
		if (zug?.gezogen) {
			return zug.start < zug.ende
				? { von: zug.start, bis: zug.ende }
				: { von: zug.ende, bis: zug.start };
		}
		return zeitraum;
	});
	const imZeitraum = (tag: string): boolean =>
		auswahl !== null && tag >= auswahl.von && tag <= auswahl.bis;

	function anzahlText(n: number): string {
		return n === 1 ? '1 Eintrag' : `${String(n)} Einträge`;
	}
	const beschriftung = $derived(
		`Einträge je Tag, ${ausschnittText(f, heute, (t) => kurzDatum(t, true))}. ` +
			'Pfeile wählen einen Tag, Bild auf und Bild ab eine Woche, Enter springt hin, ' +
			'Plus und Minus zoomen, Pos1 geht auf heute, Ende auf den ältesten Tag.'
	);
	const cursorText = $derived(
		`${c === heute ? 'Heute' : tagTitel(c, heute)}: ${anzahlText(cursorBalken?.anzahl ?? 0)}`
	);
</script>

<div
	class="dichte"
	data-zeit="dichte"
	data-von={f.von}
	data-bis={f.bis}
	data-cursor={c}
	data-auswahl-von={zeitraum?.von}
	data-auswahl-bis={zeitraum?.bis}
	role="slider"
	tabindex="0"
	aria-label={beschriftung}
	aria-valuemin={tagZahl(grenzen.von)}
	aria-valuemax={tagZahl(grenzen.bis)}
	aria-valuenow={tagZahl(c)}
	aria-valuetext={cursorText}
	{onkeydown}
>
	<!-- svelte-ignore a11y_no_static_element_interactions (Zeigen mit Maus oder Finger auf den Balken; die Tastatur bedient den Schieberegler darum herum) -->
	<div
		class="streifen"
		data-streifen
		bind:this={streifen}
		bind:clientWidth={breite}
		{onpointerdown}
		{onpointermove}
		{onpointerup}
		{onpointercancel}
	>
		{#each sichtbar as b (b.tag)}
			<span
				class="tag"
				class:cursor={b.tag === c}
				class:auswahl={imZeitraum(b.tag)}
				class:heute={b.tag === heute}
				data-tag={b.tag}
				title="{tagTitel(b.tag, heute)}: {anzahlText(b.anzahl)}"
			>
				{#if b.anzahl > 0}
					<span class="saeule" style:height="{Math.max(12, (b.anzahl / hoechstes) * 100)}%">
						{#if b.wichtig > 0}
							<span class="wichtig" style:height="{(b.wichtig / b.anzahl) * 100}%"></span>
						{/if}
					</span>
				{/if}
			</span>
		{/each}
	</div>
	{#if breite > 0}
		<Zeitachse fenster={achsenFenster} {breite} heute={tagZahl(heute) + 0.5} linienHoehe={0.25} />
	{/if}
	<p class="stand" aria-hidden="true">
		<span>{cursorText}</span>
		<span class="spanne">{laenge(f)} Tage</span>
	</p>
</div>
<!-- Etappe 3b (Fund des Prüfers „auf 4 Zeitstrahlen fehlt der Knopf“): „Heute“ als
     Knopf neben dem Schieberegler (Pos1 tut dasselbe); er springt auch im Verlauf hin. -->
<div class="leiste">
	<button
		type="button"
		class="pill"
		onclick={() => {
			fenster = null;
			waehle(heute);
		}}>Heute</button
	>
</div>

<style>
	.dichte {
		margin: 0.25rem 0 0.75rem;
		border-radius: 0.5rem;
		user-select: none;
	}
	.dichte:focus-visible {
		outline-offset: 0.25rem;
	}
	.leiste {
		display: flex;
		justify-content: flex-end;
		margin: -0.5rem 0 0.75rem;
	}
	.leiste .pill {
		min-width: 2.75rem;
		min-height: 2.75rem;
		border: 0.0625rem solid var(--line);
		cursor: pointer;
		font: inherit;
		font-size: var(--text-xs);
	}
	.streifen {
		display: flex;
		align-items: flex-end;
		height: 2.75rem;
		gap: 0.0625rem;
		touch-action: pan-y;
		cursor: pointer;
		border-bottom: 0.0625rem solid var(--line);
	}
	.tag {
		position: relative;
		flex: 1 1 0;
		min-width: 0;
		height: 100%;
		display: flex;
		align-items: flex-end;
		border-radius: 0.125rem 0.125rem 0 0;
	}
	.tag.auswahl {
		background: var(--accent-soft);
	}
	.tag.cursor {
		background: color-mix(in srgb, var(--accent) 22%, transparent);
	}
	.tag.heute::after {
		content: '';
		position: absolute;
		inset: 0 auto 0 50%;
		width: 0.125rem;
		margin-left: -0.0625rem;
		background: var(--accent);
		opacity: 0.6;
	}
	.saeule {
		position: relative;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		width: 100%;
		border-radius: 0.125rem 0.125rem 0 0;
		background: color-mix(in srgb, var(--muted) 45%, transparent);
		overflow: hidden;
	}
	.wichtig {
		display: block;
		width: 100%;
		background: var(--accent);
	}
	.stand {
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		margin: 0.125rem 0 0;
		font-size: var(--text-xs);
		color: var(--muted);
	}
	.spanne {
		white-space: nowrap;
	}
</style>
