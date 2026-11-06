<script lang="ts">
	/**
	 * Minilinie (Kopfleiste, Fassung 3): ein kleiner Verlauf als Linie, etwa
	 * die Prozessorlast der letzten Minute im PC-Symbol oben rechts. Werte von
	 * 0 bis 100, der älteste zuerst; zu wenige Werte stehen rechtsbündig (die
	 * Linie wächst von rechts herein). Die Farbe kommt aus der Bedeutung (nur
	 * Token: `b-<bedeutung>` setzt `--ton`), die Stufe wählt der Server.
	 *
	 *   <Minilinie werte={[12, 30, 18]} plaetze={12} bedeutung="ok" />
	 *
	 * Ohne `beschriftung` ist die Linie Schmuck (aria-hidden); den Wert sagt
	 * dann der Knopf oder Link, in dem sie steht. Bei reduzierter Bewegung
	 * (prefers-reduced-motion) erscheint der neueste Punkt ohne Animation.
	 *
	 * Lesbar auch bei wenig Last (Live-Probe: bei 13 %
	 * lag die Linie fast unsichtbar am unteren Rand): Die Grundlinie steht
	 * ganz unten in `--muted`, 0 % liegt `ABSTAND` darüber, damit die Linie nie
	 * auf ihr verschwindet. Die Werte bleiben ehrlich: eine lineare Skala von
	 * 0 % (knapp über der Grundlinie) bis 100 % (oben), nichts wird für wenig
	 * Last gestreckt.
	 */
	import { klasse } from '#lib/bedeutung.ts';
	import type { Bedeutung } from '$vertrag';

	interface Props {
		werte: readonly number[];
		/** So viele Plätze breit (etwa 12 für 60 s bei einem Wert alle 5 s). */
		plaetze?: number;
		bedeutung?: Bedeutung;
		/** Name für Vorlese-Programme; ohne ist die Linie nur Schmuck. */
		beschriftung?: string | undefined;
	}
	let { werte, plaetze = 12, bedeutung = 'neutral', beschriftung }: Props = $props();

	const B = 32;
	const H = 16;
	/** Die Grundlinie (Mitte des Strichs) ganz unten. */
	const GRUND = H - 0.5;
	/** So weit über der Grundlinie liegt 0 % (Strich und Grundlinie berühren sich nicht). */
	const ABSTAND = 2.5;
	/** y für 0 % und für 100 % (oben Platz für den halben Strich). */
	const NULL_Y = GRUND - ABSTAND;
	const VOLL_Y = 1;
	const n = $derived(Math.max(plaetze, werte.length, 2));
	const punkte = $derived(
		werte.slice(-n).map((v, i, liste) => {
			const x = ((i + (n - liste.length)) / (n - 1)) * B;
			// Linear: gleiche Prozente, gleiche Höhe; nichts gestreckt.
			const y = NULL_Y - (Math.max(0, Math.min(100, v)) / 100) * (NULL_Y - VOLL_Y);
			return [x, y] as const;
		})
	);
	const linie = $derived(
		punkte.map(([x, y], i) => `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`).join(' ')
	);
	const letzter = $derived(punkte.at(-1) ?? null);
</script>

<svg
	class="minilinie {klasse(bedeutung)}"
	viewBox="0 0 {B} {H}"
	width={B}
	height={H}
	role={beschriftung ? 'img' : undefined}
	aria-label={beschriftung}
	aria-hidden={beschriftung ? undefined : 'true'}
	data-werte={werte.length}
>
	<line class="grund" x1="0" x2={B} y1={GRUND} y2={GRUND} />
	{#if punkte.length > 1}<path class="linie" d={linie} />{/if}
	{#if letzter}
		{#key werte.length + (werte.at(-1) ?? 0)}
			<circle class="jetzt" cx={letzter[0]} cy={letzter[1]} r="1.75" />
		{/key}
	{/if}
</svg>

<style>
	.minilinie {
		display: block;
		flex: none;
		overflow: visible;
	}
	/* Deutlicher als --line, aber ruhig: die Grundlinie zeigt, wo 0 ist. */
	.grund {
		stroke: var(--muted);
		stroke-width: 1;
		stroke-opacity: 0.55;
	}
	.linie {
		fill: none;
		stroke: var(--ton, var(--muted));
		stroke-width: 1.75;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.jetzt {
		fill: var(--ton, var(--muted));
		animation: auf 400ms ease-out;
	}
	@keyframes auf {
		from {
			opacity: 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.jetzt {
			animation: none;
		}
	}
</style>
