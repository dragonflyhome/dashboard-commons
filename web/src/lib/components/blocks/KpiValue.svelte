<script lang="ts">
	/**
	 * Zeigt einen Kennzahl-Text. Ist er eine einzelne Zahl mit höchstens
	 * Präfix und Einheit („24“, „3,5 %“, „45 kg“, siehe `splitNumber`),
	 * zählt sie bei Änderung weich hoch oder runter; sonst steht der Text
	 * unverändert da.
	 *
	 * Ohne Animation, sofort mit dem Endwert: bei reduced-motion und wenn der
	 * Tab verborgen ist (dann steht requestAnimationFrame still, und beim
	 * Zurückwechseln stünde sonst „0 von 8“). Wird der Tab während des Zählens
	 * verborgen oder wieder sichtbar, springt der Wert ebenfalls ans Ende.
	 *
	 * Ein Fenster kann als sichtbar gelten, ohne dass requestAnimationFrame
	 * läuft (verdecktes Fenster, Hintergrund-Pane). Darum springt der Wert
	 * spätestens `DAUER + 200` ms nach dem Start per setTimeout ans Ende; ist
	 * das Zählen vorher fertig, wird die Frist gelöscht.
	 */
	import { untrack } from 'svelte';
	import { Tween } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';
	import { formatNumber, splitNumber } from '#lib/kpi.ts';

	interface Props {
		value: string;
	}
	let { value }: Props = $props();

	/** Dauer des Zählens in ms. */
	const DAUER = 700;
	/** Spätestens dann steht der Endwert da, auch ohne einen Frame. */
	const FRIST = DAUER + 200;

	const parts = $derived(splitNumber(value));
	const shown = new Tween(0, { duration: DAUER, easing: cubicOut });
	let bumped = $state(false);
	let frist: ReturnType<typeof setTimeout> | undefined;

	function fristLoeschen(): void {
		if (frist !== undefined) clearTimeout(frist);
		frist = undefined;
	}

	/** Ohne Animation, weil der Nutzer es so will oder niemand zusieht. */
	function sofort(): boolean {
		if (typeof document === 'undefined') return true;
		const reduce =
			typeof window.matchMedia === 'function' &&
			window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		return reduce || document.hidden;
	}

	/** Bricht ein laufendes Zählen ab und zeigt den Endwert. */
	function ansEnde(): void {
		fristLoeschen();
		bumped = false;
		// Dauer 0 bricht das laufende Zählen ab; ein später Frame setzt nichts mehr.
		if (shown.current !== shown.target) void shown.set(shown.target, { duration: 0 });
	}

	/** Neues Ziel: zählen oder springen. */
	function zuZiel(target: number): void {
		if (sofort()) {
			fristLoeschen();
			bumped = false;
			if (shown.target !== target || shown.current !== target) {
				void shown.set(target, { duration: 0 });
			}
			return;
		}
		if (shown.target === target) return;
		fristLoeschen();
		shown.set(target).then(
			() => {
				// Nur das jüngste Zählen räumt auf (ein älteres kann vorher enden).
				if (shown.current !== shown.target) return;
				fristLoeschen();
				bumped = false;
			},
			() => undefined
		);
		bumped = true;
		frist = setTimeout(ansEnde, FRIST);
	}

	$effect(() => {
		const target = parts.number;
		if (target === null) return;
		// Nur das Ziel löst das aus, nicht jeder Schritt des Zählens.
		untrack(() => {
			zuZiel(target);
		});
	});

	$effect(() => {
		document.addEventListener('visibilitychange', ansEnde);
		return () => {
			document.removeEventListener('visibilitychange', ansEnde);
			fristLoeschen();
		};
	});

	const text = $derived(
		parts.number === null
			? value
			: `${parts.prefix}${formatNumber(shown.current, parts.decimals)}${parts.suffix}`
	);
</script>

<span class="kv" class:bump={bumped}>{text}</span>

<style>
	.kv {
		display: inline-block;
		transition:
			transform 300ms ease,
			color 300ms ease;
	}
	.kv.bump {
		transform: scale(1.06);
		color: var(--accent);
	}
</style>
