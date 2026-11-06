<script lang="ts">
	import type { FlowData } from '$vertrag';
	import { SvelteSet } from 'svelte/reactivity';
	import { flussText } from '#lib/textfassung.ts';
	import Textfassung from '../Textfassung.svelte';

	interface Props {
		data: FlowData;
		/** Textfassung gleich sichtbar (Katalog). */
		textOffen?: boolean;
	}
	let { data, textOffen = false }: Props = $props();

	const bw = 150;
	const bh = 40;
	const gx = 60;
	const gy = 24;

	const maxLayer = $derived(Math.max(0, ...data.nodes.map((n) => n.layer)));
	const perLayer = $derived.by(() => {
		const counts = new Array<number>(maxLayer + 1).fill(0);
		for (const n of data.nodes) counts[n.layer] = (counts[n.layer] ?? 0) + 1;
		return counts;
	});
	const rowsMax = $derived(Math.max(1, ...perLayer));
	const width = $derived((maxLayer + 1) * (bw + gx) - gx + 4);
	const height = $derived(rowsMax * (bh + gy) - gy + 4);

	function pos(i: number): { x: number; y: number } {
		const n = data.nodes[i];
		if (!n) return { x: 0, y: 0 };
		const count = perLayer[n.layer] ?? 1;
		const offset = ((rowsMax - count) * (bh + gy)) / 2;
		return { x: 2 + n.layer * (bw + gx), y: 2 + offset + n.slot * (bh + gy) };
	}

	/** Beschriftung auf höchstens zwei Zeilen, wie im Rust-Renderer. */
	function wrap(s: string, w = 20): string[] {
		if (s.length <= w) return [s];
		const words = s.split(/\s+/);
		let first = '';
		const rest: string[] = [];
		for (const word of words) {
			if (rest.length === 0 && (first.length + word.length + 1 <= w || first === '')) {
				first = first === '' ? word : `${first} ${word}`;
			} else rest.push(word);
		}
		const cut = (t: string): string => (t.length <= w + 2 ? t : `${t.slice(0, w + 1)}…`);
		return rest.length === 0 ? [cut(first)] : [cut(first), cut(rest.join(' '))];
	}

	/** Hervorhebung: gewählter Knoten plus direkte Nachbarn. */
	let pinned = $state<number | null>(null);
	let hovered = $state<number | null>(null);
	const focus = $derived(hovered ?? pinned);
	const related = $derived.by(() => {
		const s = new SvelteSet<number>();
		if (focus === null) return s;
		s.add(focus);
		for (const [a, b] of data.edges) {
			if (a === focus) s.add(b);
			if (b === focus) s.add(a);
		}
		return s;
	});
	function edgeActive(a: number, b: number): boolean {
		return focus !== null && (a === focus || b === focus);
	}
	function path(a: number, b: number): string {
		const pa = pos(a);
		const pb = pos(b);
		const x1 = pa.x + bw;
		const y1 = pa.y + bh / 2;
		const x2 = pb.x;
		const y2 = pb.y + bh / 2;
		const c = Math.max(Math.abs(x2 - x1), 30) / 2;
		return `M ${x1} ${y1} C ${x1 + c} ${y1}, ${x2 - c} ${y2}, ${x2} ${y2}`;
	}
	const uid = $props.id();
</script>

<div class="flow" class:focused={focus !== null}>
	<svg
		viewBox="0 0 {width} {height}"
		preserveAspectRatio="xMinYMin meet"
		role="img"
		aria-label="Flussdiagramm mit {data.nodes.length} Kästen"
	>
		<defs>
			<marker
				id="arrow-{uid}"
				viewBox="0 0 10 10"
				refX="9"
				refY="5"
				markerWidth="7"
				markerHeight="7"
				orient="auto-start-reverse"
			>
				<path d="M 0 0 L 10 5 L 0 10 z" />
			</marker>
		</defs>
		{#each data.edges as [a, b], i (i)}
			<path
				class="edge"
				class:active={edgeActive(a, b)}
				d={path(a, b)}
				marker-end="url(#arrow-{uid})"
			/>
		{/each}
		{#each data.nodes as n, i (n.id)}
			{@const p = pos(i)}
			{@const lines = wrap(n.label)}
			<g
				class="node"
				class:active={related.has(i)}
				class:pinned={pinned === i}
				role="button"
				tabindex="0"
				aria-pressed={pinned === i}
				aria-label={n.label}
				onpointerenter={() => {
					hovered = i;
				}}
				onpointerleave={() => {
					hovered = null;
				}}
				onfocus={() => {
					hovered = i;
				}}
				onblur={() => {
					hovered = null;
				}}
				onclick={() => {
					pinned = pinned === i ? null : i;
				}}
				onkeydown={(e) => {
					if (e.key === 'Enter' || e.key === ' ') {
						e.preventDefault();
						pinned = pinned === i ? null : i;
					}
				}}
			>
				<rect x={p.x} y={p.y} width={bw} height={bh} rx="8" />
				{#if lines.length === 1}
					<text x={p.x + bw / 2} y={p.y + bh / 2 + 5} text-anchor="middle">{lines[0]}</text>
				{:else}
					<text x={p.x + bw / 2} y={p.y + bh / 2 - 3} text-anchor="middle">
						<tspan x={p.x + bw / 2}>{lines[0]}</tspan>
						<tspan x={p.x + bw / 2} dy="15">{lines[1]}</tspan>
					</text>
				{/if}
			</g>
		{/each}
	</svg>
	<p class="muted small hint">
		Kasten berühren hebt Verbindungen hervor, Klick hält die Auswahl fest.
	</p>
	<!-- Etappe 3b: Textfassung, je Kasten seine Pfeile. -->
	<Textfassung fassung={flussText(data)} offen={textOffen} />
</div>

<style>
	.flow {
		margin: 0.6em 0 1em;
	}
	svg {
		display: block;
		width: 100%;
		max-width: 100%;
		height: auto;
		max-height: 420px;
		overflow: visible;
	}
	.edge {
		fill: none;
		stroke: var(--muted);
		stroke-width: 1.6;
		transition:
			opacity 150ms ease,
			stroke-width 150ms ease;
	}
	.focused .edge {
		opacity: 0.25;
	}
	.focused .edge.active {
		opacity: 1;
		stroke: var(--accent);
		stroke-width: 2.4;
	}
	marker path {
		fill: var(--muted);
	}
	.node {
		cursor: pointer;
		outline: none;
	}
	.node rect {
		fill: var(--card);
		stroke: var(--line);
		stroke-width: 1.5;
		transition:
			stroke 150ms ease,
			fill 150ms ease,
			opacity 150ms ease;
	}
	.node text {
		font-size: var(--text-grafik-klein);
		fill: var(--text);
		pointer-events: none;
	}
	.focused .node {
		opacity: 0.45;
	}
	.focused .node.active {
		opacity: 1;
	}
	.node.active rect,
	.node:focus-visible rect {
		stroke: var(--accent);
		stroke-width: 2.5;
		fill: var(--accent-soft);
	}
	.node.pinned rect {
		stroke-dasharray: 5 3;
	}
	.hint {
		margin: 2px 0 0;
	}
</style>
