<script lang="ts" module>
	import type { Fenster as AnsichtFenster, Stufe as AnsichtStufe } from '#lib/zeit/achse.ts';

	/** Stufe und Ausschnitt je Plan, solange die Seite offen ist (über Reiterwechsel hinweg). */
	// eslint-disable-next-line svelte/prefer-svelte-reactivity -- nur ein Gedächtnis, das Zeichnen hängt nicht daran
	const ansichten = new Map<
		string,
		{ wahl: 'alles' | AnsichtStufe; fenster: AnsichtFenster | null }
	>();
</script>

<script lang="ts">
	/**
	 * Balkenplan auf der gemeinsamen Zeitachse (Etappe 3a-1). Zoomstufen Alles,
	 * Jahr, Monat, Woche und Tag (bei Plan-Monaten Alles, Jahr, Monat), dazu
	 * die Heute-Linie. Bedienung wie im Dichtestreifen: Pfeile verschieben um
	 * eine Einheit der Stufe, + und - zoomen, Pos1 springt auf heute, 0 zeigt
	 * alles; mit Maus oder Finger ziehen verschiebt, Strg + Mausrad zoomt.
	 * Kein Alt+Pfeil. Am Handy steht der Name über dem Balken, damit nichts
	 * seitlich scrollt.
	 */
	import type { Bedeutung, GanttBar, GanttData } from '$vertrag';
	import { SvelteSet } from 'svelte/reactivity';
	import { klasse, wort } from '#lib/bedeutung.ts';
	import StatusDot from '#lib/components/StatusDot.svelte';
	import { EPOCH_DAYS, fmtDuration } from '#lib/timescale.ts';
	import Zeitachse from './Zeitachse.svelte';
	import {
		STUFEN_NAME,
		begrenzen,
		datumAusZahl,
		fensterFuerStufe,
		gepolstert,
		position,
		schrittFuer,
		spanne,
		tagzahlAus,
		verschieben,
		type Fenster,
		type Stufe
	} from '#lib/zeit/achse.ts';

	interface Props {
		data: GanttData;
		/** Für Tests; sonst jetzt. */
		jetzt?: Date;
	}
	let { data, jetzt = new Date() }: Props = $props();

	type Wahl = 'alles' | Stufe;

	const plan = $derived(!data.dates);
	/** Werte des Servers (Tage seit 1.1.1 oder Plan-Monate) in Achsen-Einheiten. */
	const einheit = (v: number): number => (data.dates ? v - EPOCH_DAYS : v);
	const grenzen = $derived<Fenster>(
		gepolstert({ von: einheit(data.min), bis: Math.max(einheit(data.max), einheit(data.min) + 1) })
	);
	const heute = $derived(plan ? null : tagzahlAus(jetzt));
	const heuteDrin = $derived(heute !== null && heute >= grenzen.von && heute <= grenzen.bis);

	const farben = $derived([...new Set(data.bars.map((b) => b.bedeutung))]);
	const aus = new SvelteSet<Bedeutung>();
	const balken = $derived(data.bars.filter((b) => !aus.has(b.bedeutung)));

	let gesamtBreite = $state(0);
	let breite = $state(0);
	const schmal = $derived(gesamtBreite > 0 && gesamtBreite < 640);

	let wahl = $state<Wahl>('alles');
	let fenster = $state<Fenster | null>(null);
	const f = $derived(fenster ?? grenzen);

	/** Welche Stufen es gibt: nur, wenn ihr Ausschnitt kürzer ist als das Ganze. */
	const stufen = $derived.by((): Wahl[] => {
		const moeglich: Stufe[] = plan ? ['jahr', 'monat'] : ['jahr', 'monat', 'woche', 'tag'];
		const b = Math.max(breite, 200);
		const passend = moeglich.filter(
			(s) => spanne(fensterFuerStufe(s, b, 0, plan)) < spanne(grenzen)
		);
		// Auch ein Plan von wenigen Tagen lässt sich vergrößern: dann die feinste Stufe.
		const feinste = moeglich[moeglich.length - 1];
		return ['alles', ...(passend.length === 0 && feinste !== undefined ? [feinste] : passend)];
	});

	function mitte(): number {
		return f.von + spanne(f) / 2;
	}
	/**
	 * Um welchen Punkt gezoomt wird (Etappe 3b, Fund des Prüfers „＋ zoomt um
	 * die Mitte, Neues fällt heraus“): heute, solange heute im Bild ist, sonst
	 * die Mitte. Strg + Mausrad zoomt um die Stelle unter dem Zeiger.
	 */
	function zielpunkt(): number {
		return heute !== null && heute >= f.von && heute <= f.bis ? heute : mitte();
	}
	/** `um` steht nach dem Zoomen an der Stelle `anteil` (0 links, 1 rechts). */
	function setzen(w: Wahl, um = zielpunkt(), anteil = 0.5): void {
		wahl = w;
		if (w === 'alles') {
			fenster = null;
			merken();
			return;
		}
		let neu = fensterFuerStufe(w, Math.max(breite, 200), um, plan);
		// Kurzer Plan (ein paar Tage): die feinste Stufe zeigt die Hälfte des Ganzen.
		if (spanne(neu) >= spanne(grenzen)) {
			const s = spanne(grenzen) / 2;
			neu = { von: um - s / 2, bis: um + s / 2 };
		}
		const s = spanne(neu);
		neu = { von: um - anteil * s, bis: um + (1 - anteil) * s };
		fenster = begrenzen(neu, grenzen, fenster ?? undefined);
		merken();
	}
	function zoom(richtung: 1 | -1, um?: number, anteil?: number): void {
		const i = stufen.indexOf(wahl);
		const j = Math.min(Math.max(i + richtung, 0), stufen.length - 1);
		setzen(stufen[j] ?? 'alles', um, anteil);
	}

	/**
	 * Zoom über einen Reiterwechsel hinweg (Etappe 3b, Fund des Prüfers): Der
	 * Reiter baut seinen Inhalt neu, also merkt sich der Balkenplan Stufe und
	 * Ausschnitt je Plan (`schluessel`) für die Dauer des Besuchs.
	 */
	const schluessel = $derived(
		JSON.stringify([data.min, data.max, data.dates, data.bars.map((b) => [b.name, b.start, b.end])])
	);
	function merken(): void {
		ansichten.set(schluessel, { wahl, fenster });
	}
	let geholt = false;
	$effect.pre(() => {
		if (geholt) return;
		geholt = true;
		const a = ansichten.get(schluessel);
		if (a !== undefined) {
			wahl = a.wahl;
			fenster = a.fenster;
		}
	});
	function schieben(einheiten: number): void {
		if (wahl === 'alles') return;
		fenster = begrenzen(verschieben(f, einheiten * schrittFuer(wahl, plan)), grenzen, f);
		merken();
	}
	function aufHeute(): void {
		if (heute === null || !heuteDrin) return;
		if (wahl === 'alles') setzen(stufen[Math.min(2, stufen.length - 1)] ?? 'alles', heute);
		else setzen(wahl, heute);
	}

	function onkeydown(e: KeyboardEvent): void {
		if (e.altKey || e.metaKey || e.ctrlKey) return;
		switch (e.key) {
			case 'ArrowLeft':
				schieben(-1);
				break;
			case 'ArrowRight':
				schieben(1);
				break;
			case '+':
			case '=':
				zoom(1);
				break;
			case '-':
				zoom(-1);
				break;
			case '0':
				setzen('alles');
				break;
			case 'Home':
				aufHeute();
				break;
			default:
				return;
		}
		e.preventDefault();
	}
	function onwheel(e: WheelEvent): void {
		// Nur mit Strg (oder Cmd, auch Zwei-Finger-Pinch); sonst rollt die Seite.
		if (!e.ctrlKey && !e.metaKey) return;
		e.preventDefault();
		// Um die Stelle unter dem Zeiger: sie bleibt, wo sie ist.
		const r = kopfSpur?.getBoundingClientRect();
		const x = r === undefined ? -1 : e.clientX - r.left;
		if (r !== undefined && breite > 0 && x >= 0 && x <= breite) {
			zoom(e.deltaY < 0 ? 1 : -1, f.von + (x / breite) * spanne(f), x / breite);
		} else {
			zoom(e.deltaY < 0 ? 1 : -1);
		}
	}
	let kopfSpur = $state<HTMLDivElement | null>(null);
	let zug = $state<{ x: number; von: Fenster } | null>(null);
	function onpointerdown(e: PointerEvent): void {
		if (e.button !== 0 || wahl === 'alles') return;
		zug = { x: e.clientX, von: f };
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
	}
	function onpointermove(e: PointerEvent): void {
		if (zug === null || breite <= 0) return;
		const delta = ((zug.x - e.clientX) / breite) * spanne(zug.von);
		fenster = begrenzen(verschieben(zug.von, delta), grenzen, f);
	}
	function onpointerup(): void {
		if (zug !== null) merken();
		zug = null;
	}

	function lage(b: GanttBar): { links: number; breite: number } | null {
		const a = einheit(b.start);
		const z = einheit(b.end);
		if (z < f.von || a > f.bis) return null;
		const x1 = Math.max(position(f, breite, a), 0);
		const x2 = Math.min(position(f, breite, z), breite);
		return { links: x1, breite: Math.max(x2 - x1, 3) };
	}
	function zeitraum(b: GanttBar): string {
		const von = data.dates ? datumAusZahl(einheit(b.start)) : `Monat ${String(b.start)}`;
		const bis = data.dates ? datumAusZahl(einheit(b.end)) : `Monat ${String(b.end)}`;
		return `${von} bis ${bis} · ${fmtDuration(b.end - b.start, data.dates)}`;
	}
	const bereich = $derived(
		plan
			? `Monat ${String(Math.max(Math.round(f.von), 0))} bis ${String(Math.round(f.bis))}`
			: `${datumAusZahl(Math.ceil(f.von))} bis ${datumAusZahl(Math.floor(f.bis))}`
	);
	const uid = $props.id();
	const achsenStufe = $derived<Stufe | undefined>(wahl === 'alles' ? undefined : wahl);
	const heuteX = $derived(
		heute === null || heute < f.von || heute > f.bis ? null : position(f, breite, heute)
	);
</script>

<div
	class="gantt"
	class:schmal
	data-zeit="gantt"
	data-stufe={wahl}
	data-von={f.von.toFixed(2)}
	data-bis={f.bis.toFixed(2)}
	data-heute-ausserhalb={heuteDrin ? undefined : 'ja'}
	bind:clientWidth={gesamtBreite}
>
	<div class="leiste">
		<div class="stufen" role="group" aria-label="Zoom">
			{#each stufen as s (s)}
				<button
					type="button"
					class="pill"
					aria-pressed={wahl === s}
					onclick={() => {
						setzen(s);
					}}>{s === 'alles' ? 'Alles' : STUFEN_NAME[s]}</button
				>
			{/each}
			{#if heuteDrin}
				<button type="button" class="pill" onclick={aufHeute}>Heute</button>
			{/if}
		</div>
		<span class="bereich" id="bereich-{uid}" aria-live="polite">{bereich}</span>
	</div>
	{#if farben.length > 1}
		<div class="filter" role="group" aria-label="Nach Status filtern">
			{#each farben as c (c)}
				<button
					type="button"
					class="pill"
					aria-pressed={!aus.has(c)}
					onclick={() => {
						if (aus.has(c)) aus.delete(c);
						else aus.add(c);
					}}
				>
					<StatusDot bedeutung={c} decorative />
					{wort(c)}
				</button>
			{/each}
		</div>
	{/if}
	<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (die Fläche ist als role=application bewusst bedienbar: Tastatur, Ziehen und Strg+Rad steuern den Ausschnitt; die Knöpfe darüber bieten denselben Weg) -->
	<div
		class="buehne"
		class:zieht={zug !== null}
		class:beweglich={wahl !== 'alles'}
		data-zeit-buehne
		role="application"
		tabindex="0"
		aria-label="Balkenplan. Pfeile verschieben, Plus und Minus zoomen, Pos1 springt auf heute, 0 zeigt alles."
		aria-describedby="bereich-{uid}"
		{onkeydown}
		{onwheel}
		{onpointerdown}
		{onpointermove}
		{onpointerup}
		onpointercancel={onpointerup}
	>
		<div class="reihe kopfzeile">
			<span class="name"></span>
			<div class="spur" bind:clientWidth={breite} bind:this={kopfSpur}>
				{#if breite > 0}
					<Zeitachse
						fenster={f}
						{breite}
						stufe={achsenStufe}
						{plan}
						heute={heuteDrin ? heute : null}
					/>
				{/if}
			</div>
		</div>
		<ul class="reihen">
			{#each balken as b, i (`${b.name}-${String(i)}`)}
				{@const l = breite > 0 ? lage(b) : null}
				<li class="reihe">
					<span class="name" data-balken-name title={b.name}>{b.name}</span>
					<div class="spur">
						{#if heuteX !== null}
							<span class="heute" style:left="{heuteX}px" aria-hidden="true"></span>
						{/if}
						{#if l !== null}
							<span
								class="balken {klasse(b.bedeutung)}"
								style:left="{l.links}px"
								style:width="{l.breite}px"
								title="{b.name}: {zeitraum(b)}"
							></span>
						{/if}
						<span class="unsichtbar">{zeitraum(b)}, {wort(b.bedeutung)}</span>
					</div>
				</li>
			{/each}
		</ul>
	</div>
</div>

<style>
	.gantt {
		margin: 0.6rem 0 1rem;
	}
	.leiste {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 0.375rem 0.75rem;
		margin-bottom: 0.375rem;
	}
	.stufen,
	.filter {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
	}
	.filter {
		margin-bottom: 0.375rem;
	}
	.pill {
		border: 0.0625rem solid var(--line);
		cursor: pointer;
		font: inherit;
		font-size: var(--text-xs);
	}
	/* Handy (Etappe 3b, Fund des Prüfers „Knöpfe 28 px“): Ziele mindestens 44 px. */
	.schmal .pill {
		min-width: 2.75rem;
		min-height: 2.75rem;
	}
	.stufen .pill[aria-pressed='true'] {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--on-accent);
	}
	.filter .pill[aria-pressed='false'] {
		opacity: 0.5;
		text-decoration: line-through;
	}
	.bereich {
		font-size: var(--text-xs);
		color: var(--muted);
	}
	.buehne {
		border-radius: 0.5rem;
		touch-action: pan-y;
		user-select: none;
	}
	.buehne.beweglich {
		cursor: grab;
	}
	.buehne.zieht {
		cursor: grabbing;
	}
	.reihen {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.reihe {
		display: grid;
		grid-template-columns: 12rem minmax(0, 1fr);
		align-items: center;
		gap: 0 0.5rem;
		min-height: 1.75rem;
	}
	.name {
		font-size: var(--text-xs);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.spur {
		position: relative;
		height: 1.5rem;
		overflow: hidden;
	}
	.kopfzeile .spur {
		height: auto;
	}
	.reihen .spur {
		background: linear-gradient(var(--line), var(--line)) center / 100% 0.0625rem no-repeat;
	}
	.balken {
		position: absolute;
		top: 0.25rem;
		bottom: 0.25rem;
		border-radius: 0.3rem;
		/* Ton aus der Klasse der Bedeutung (app.css, bedeutung.ts). */
		background: var(--ton, var(--plan));
	}
	.heute {
		position: absolute;
		top: 0;
		bottom: 0;
		width: 0.125rem;
		margin-left: -0.0625rem;
		background: var(--accent);
		opacity: 0.5;
	}
	.unsichtbar {
		position: absolute;
		width: 0.0625rem;
		height: 0.0625rem;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
	/* Handy: der Name über dem Balken, volle Breite für die Zeit. */
	.schmal .reihe {
		grid-template-columns: minmax(0, 1fr);
	}
	.schmal .kopfzeile .name {
		display: none;
	}
	.schmal .name {
		margin-top: 0.25rem;
	}
	@media (prefers-reduced-motion: no-preference) {
		.balken {
			transition:
				left 150ms ease,
				width 150ms ease;
		}
	}
</style>
