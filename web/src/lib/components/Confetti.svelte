<script lang="ts">
	/**
	 * Kleiner Erfolgsmoment: ein paar bunte Schnipsel rieseln kurz herab.
	 * Sparsam einsetzen; bei reduced-motion passiert nichts.
	 */
	import { reihe } from '#lib/reihen.ts';

	interface Props {
		/** Steigt der Wert, rieselt es erneut (nie bei gleichem oder kleinerem Wert). */
		trigger?: number;
		/**
		 * Schon beim ersten Zeichnen rieseln, wenn trigger dann größer als 0 ist
		 * (etwa /schluessel nach der Umleitung ?gespeichert=…).
		 */
		sofort?: boolean;
	}
	let { trigger = 0, sofort = false }: Props = $props();

	interface Piece {
		x: number;
		delay: number;
		/** Farbe aus den Reihen-Token (app.css). */
		farbe: string;
		rot: number;
		size: number;
	}
	let pieces = $state<Piece[]>([]);
	// Nur ein Anstieg von trigger lässt es rieseln, nicht schon das erste
	// Zeichnen der Seite (vorher feuerte es bei jedem Aufruf: 0 gegen -1) und
	// nicht ein gleicher oder kleinerer Wert (Auflage Hotfix 3a-1).
	// Kein $state: sonst liefe der Effekt nach dem Merken noch einmal.
	let last: number | null = null;
	// Etappe 3b (Fund des Prüfers „Konfetti 48 dauerhaft im Speicher“): Das
	// Aufräumen hing am Effekt; lief er vor Ablauf noch einmal ohne zu
	// rieseln (gleicher oder kleinerer Wert), löschte das den Zeitgeber, und die
	// 48 Stücke blieben für immer im Dokument. Jetzt hat das Aufräumen einen
	// eigenen Zeitgeber, der nur beim nächsten Rieseln oder beim Abbau weicht.
	let aufraeumen: ReturnType<typeof setTimeout> | null = null;
	function weg(): void {
		if (aufraeumen !== null) clearTimeout(aufraeumen);
		aufraeumen = null;
	}
	$effect(() => {
		const jetzt = trigger;
		const rieseln = last === null ? sofort && jetzt > 0 : jetzt > last;
		last = jetzt;
		if (!rieseln) return;
		// Erst beim Rieseln fragen, damit es nie vor dem Wissen um reduced-motion losgeht.
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		pieces = Array.from({ length: 48 }, (_, i) => ({
			x: (i / 48) * 100 + Math.random() * 2,
			delay: Math.random() * 600,
			farbe: reihe(Math.floor(Math.random() * 10)),
			rot: Math.random() * 360,
			size: 6 + Math.random() * 6
		}));
		weg();
		aufraeumen = setTimeout(() => {
			aufraeumen = null;
			pieces = [];
		}, 3200);
	});
	// Beim Abbau der Seite: Zeitgeber weg (die Stücke gehen mit dem Baustein).
	$effect(() => weg);
</script>

{#if pieces.length > 0}
	<div class="confetti" aria-hidden="true">
		{#each pieces as p, i (i)}
			<span
				style:left="{p.x}%"
				style:animation-delay="{p.delay}ms"
				style:background={p.farbe}
				style:width="{p.size}px"
				style:height="{p.size * 0.6}px"
				style:transform="rotate({p.rot}deg)"
			></span>
		{/each}
	</div>
{/if}

<style>
	.confetti {
		position: fixed;
		inset: 0;
		pointer-events: none;
		overflow: hidden;
		z-index: 50;
	}
	span {
		position: absolute;
		top: -12px;
		border-radius: 2px;
		animation: fall 2.6s cubic-bezier(0.2, 0.6, 0.4, 1) forwards;
	}
	@keyframes fall {
		0% {
			transform: translateY(0) rotate(0deg);
			opacity: 1;
		}
		100% {
			transform: translateY(105vh) rotate(720deg);
			opacity: 0;
		}
	}
</style>
