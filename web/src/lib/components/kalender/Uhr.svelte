<script lang="ts">
	/**
	 * Kalender, Stufe 7b (/heute): die Uhr als Text, kein Bild. Stunden und Minuten in
	 * der Anzeigezone (`zeit/zone.svelte.ts`, sonst Gerätezeit), darunter Datum und Ort.
	 * Ein eigener Takt nur hier: bis zur vollen Minute, dann jede Minute, und beim
	 * Wiederkehren der Seite sofort. Kein `aria-live`: Vorlese-Programme sollen nicht
	 * jede Minute ansagen.
	 */
	import { onMount } from 'svelte';
	import { anzeigeZone } from '#lib/zeit/zone.svelte.ts';

	interface Props {
		/** Der Tag am Blickort (JJJJ-MM-TT) aus dem Kopf. */
		datum: string;
		ort: string;
		/** Bekommt „HH:MM“ bei jedem neuen Stand der Uhr, für den Sonnenbogen nebenan. */
		onuhrzeit?: ((uhrzeit: string) => void) | undefined;
	}
	let { datum, ort, onuhrzeit }: Props = $props();

	let jetzt = $state(new Date());

	onMount(() => {
		let takt: ReturnType<typeof setInterval> | undefined;
		const ticken = (): void => {
			jetzt = new Date();
		};
		const ersterSchlag = setTimeout(
			() => {
				ticken();
				takt = setInterval(ticken, 60_000);
			},
			60_000 - (Date.now() % 60_000)
		);
		const sichtbar = (): void => {
			if (document.visibilityState === 'visible') ticken();
		};
		document.addEventListener('visibilitychange', sichtbar);
		return () => {
			clearTimeout(ersterSchlag);
			clearInterval(takt);
			document.removeEventListener('visibilitychange', sichtbar);
		};
	});

	/** Stunden und Minuten; kennt der Browser die Zone nicht, in Gerätezeit statt gar nicht. */
	function hhmm(d: Date, zone: string | null): string {
		const art = { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' } as const;
		try {
			return new Intl.DateTimeFormat('de-DE', { ...art, timeZone: zone ?? undefined }).format(d);
		} catch {
			return new Intl.DateTimeFormat('de-DE', art).format(d);
		}
	}
	const text = $derived(hhmm(jetzt, anzeigeZone()));
	$effect(() => {
		onuhrzeit?.(text);
	});

	/** „Freitag, 27. November 2026“ aus dem Tag des Servers (ohne Umrechnung). */
	const tagText = $derived.by(() => {
		const d = new Date(`${datum}T12:00:00Z`);
		if (Number.isNaN(d.getTime())) return datum;
		return new Intl.DateTimeFormat('de-DE', {
			weekday: 'long',
			day: 'numeric',
			month: 'long',
			year: 'numeric',
			timeZone: 'UTC'
		}).format(d);
	});
</script>

<div class="uhr" data-uhr>
	<time class="zeit" datetime={jetzt.toISOString().slice(0, 16) + 'Z'}>{text}</time>
	<p class="tag">{tagText} · {ort}</p>
</div>

<style>
	.zeit {
		display: block;
		font-size: var(--text-riesig);
		font-weight: 600;
		line-height: 1.1;
		font-variant-numeric: tabular-nums;
	}
	.tag {
		margin: 0.25rem 0 0;
		color: var(--vordergrund-leise);
	}
</style>
