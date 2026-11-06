<script lang="ts">
	/**
	 * Das PC-Symbol oben rechts (Kopfleiste, Fassung 3): die Prozessorlast der
	 * letzten etwa 60 s als kleine Linie (`Minilinie` aus dem Katalog), ein
	 * Link zu /pc. Die Werte kommen über den einen Kanal der Seite (frei oder
	 * geschützt; Prozessorlast ist ein freier Wert), also auch ohne Anmeldung.
	 * Die Farbe folgt der Stufe vom Server (normal, hoch, kritisch). Name für
	 * Vorlese-Programme „PC: Prozessor N %“, derselbe Satz im Titel.
	 *
	 * `minute` nur für Tests; sonst die eine für die Seite (`pcMinute`).
	 */
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { Minilinie } from '$katalog';
	import { belegungBedeutung } from '#lib/pc.ts';
	import { PLAETZE, pcMinute, pcName, type PcMinute } from '#lib/pcminute.svelte.ts';

	interface Props {
		minute?: PcMinute;
	}
	let { minute = pcMinute }: Props = $props();

	const name = $derived(pcName(minute.jetzt));
	const hier = $derived(page.route.id === '/pc');
</script>

<a
	class="pc"
	href={resolve('/pc')}
	aria-label={name}
	title={name}
	aria-current={hier ? 'page' : undefined}
	data-belegung={minute.belegung}
>
	<Minilinie
		werte={minute.werte}
		plaetze={PLAETZE}
		bedeutung={belegungBedeutung(minute.belegung)}
	/>
	<span class="wert" aria-hidden="true">{minute.jetzt === null ? 'PC' : `${minute.jetzt} %`}</span>
</a>

<style>
	.pc {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 0.375rem;
		min-width: 2.75rem;
		min-height: 2.75rem;
		padding: 0 0.375rem;
		border-radius: 0.625rem;
		color: var(--muted);
		font-size: var(--text-xs);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		text-decoration: none;
	}
	.pc:hover {
		background: var(--accent-soft);
		color: var(--text);
	}
	.pc[aria-current='page'] {
		background: var(--accent-soft);
		color: var(--accent-text);
	}
	/* Am Handy nur die Linie (der Wert steht im Namen und im Titel). */
	@media (max-width: 37.5rem) {
		.wert {
			display: none;
		}
	}
</style>
