<script lang="ts">
	/**
	 * Hat ein Projekt keine eigene Seite im Bau (keine Seite.svelte, oder die
	 * Prüfung hat sie wegen Fehlern ausgelassen), zeigt das Dashboard einen
	 * einfachen Aufbau: offene Punkte, jede Datei aus `daten/` mit dem
	 * passenden Baustein, Links. Darüber steht, warum.
	 */
	import type { Befund, ProjektAntwort } from '$vertrag';
	import { Abschnitt, Hinweis, Liste } from '$katalog';
	import { api } from '#lib/api/client.ts';
	import { betrifftProjekt, live, type InhaltAenderung } from '#lib/live.svelte.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import OldLink from '../OldLink.svelte';
	import { BROWSER_ADRESSE } from '#lib/anmeldung.ts';
	import StandardDatei from './StandardDatei.svelte';
	import LinksListe from './LinksListe.svelte';

	interface Props {
		kopf: ProjektAntwort;
		/** Für Unterseiten: welche Datei fehlt (Standard: Seite.svelte). */
		datei?: string | undefined;
	}
	let { kopf, datei = 'Seite.svelte' }: Props = $props();

	/** Fehler der Prüfung in Svelte- und TS-Dateien: deshalb fehlt die Seite im Bau. */
	let fehler = $state<Befund[]>([]);
	/** Etappe B2: ohne Anmeldung kommen keine Befund-Sätze, nur die Zahl der Fehler. */
	let fehlerZahl = $state(0);
	$effect(() => {
		const slug = kopf.slug;
		let aktuell = true;
		/** Laufende Nummer: eine späte Antwort überschreibt nie eine neuere. */
		let nr = 0;
		const holen = (): void => {
			const meine = ++nr;
			api.pruefung(slug).then(
				(p) => {
					if (aktuell && meine === nr) {
						fehler = p.befunde.filter(
							(b) => b.schwere === 'fehler' && /\.(svelte|ts)$/.test(b.datei)
						);
						fehlerZahl = p.befunde.length > 0 ? 0 : (p.fehler ?? 0);
					}
				},
				() => {
					// Ohne Prüfbericht bleibt der allgemeine Hinweis.
				}
			);
		};
		holen();
		// Live-Lücken: neu prüfen, wenn sich das Projekt ändert (auch Git-Stand, also
		// eine geänderte Seite.svelte), nach einer neuen Fassung und nach dem Nachholen.
		const beiInhalt = (a: InhaltAenderung): void => {
			if (betrifftProjekt(a, slug)) holen();
		};
		const aus = [
			live.onInhalt(beiInhalt),
			freierKanal.onInhalt(beiInhalt),
			freierKanal.onFassung(holen),
			live.onNachholen(holen)
		];
		return () => {
			aktuell = false;
			for (const ab of aus) ab();
		};
	});

	const titel: Record<string, string> = {
		kennzahlen: 'Kennzahlen',
		ampel: 'Ampel',
		fortschritt: 'Fortschritt',
		zeitstrahl: 'Zeitstrahl',
		gantt: 'Balkenplan',
		kanban: 'Kanban',
		fluss: 'Ablauf',
		fragen: 'Fragen an dich',
		orte: 'Orte auf diesem PC',
		fristen: 'Fristen'
	};
	function ueberschrift(name: string): string {
		return titel[name] ?? `Eigene Daten: ${name.replace(/^eigen-/, '')}`;
	}
	/** Reihenfolge wie im Katalog, eigene Daten danach (nach Namen). */
	const reihe = Object.keys(titel);
	const namen = $derived(
		[...kopf.daten].sort((a, b) => {
			const ia = reihe.indexOf(a);
			const ib = reihe.indexOf(b);
			return (ia < 0 ? reihe.length : ia) - (ib < 0 ? reihe.length : ib) || a.localeCompare(b);
		})
	);
	const pruefAdresse = $derived(`/api/pruefung/${encodeURIComponent(kopf.slug)}`);
</script>

<!-- Merkmal für Proben und Tests: hier steht der Standardaufbau, keine eigene Seite. -->
<div class="standardaufbau" data-standardaufbau>
	<Hinweis bedeutung={fehler.length > 0 || fehlerZahl > 0 ? 'problem' : 'neutral'}>
		{#if fehler.length === 0 && fehlerZahl > 0}
			<p class="kopf">
				<b>Die Prüfung meldet {fehlerZahl} Fehler.</b> Welche, siehst du mit Anmeldung (in Microsoft
				Edge unter {BROWSER_ADRESSE}). Bis der Chat sie behebt, zeigt das Dashboard die Daten im
				einfachen Aufbau.
			</p>
		{:else if fehler.length > 0}
			<p class="kopf">
				<b>{datei} ist nicht im Bau, weil die Prüfung Fehler meldet.</b> Bis der Chat sie behebt, zeigt
				das Dashboard die Daten im einfachen Aufbau.
			</p>
			<ul class="befunde small">
				{#each fehler as b, i (i)}
					<li><code>{b.datei}{b.zeile === null ? '' : `, Zeile ${b.zeile}`}</code>: {b.text}</li>
				{/each}
			</ul>
		{:else}
			<p class="kopf">
				<b>Dieses Projekt hat noch keine eigene Seite ({datei}) im Bau.</b> Bis dahin zeigt das Dashboard
				die Daten im einfachen Aufbau. Fehlt sie, weil die Prüfung sie ausgelassen hat, steht der Grund
				im Prüfbericht.
			</p>
		{/if}
		<p class="small"><OldLink href={pruefAdresse}>Prüfbericht dieses Projekts</OldLink></p>
	</Hinweis>

	{#if kopf.kopf.open.length > 0}
		<Abschnitt titel="Offene Punkte">
			<Liste punkte={kopf.kopf.open} nummeriert />
		</Abschnitt>
	{:else if (kopf.open_zahl ?? 0) > 0}
		<!-- Etappe B2: ohne Anmeldung nur die Zahl der offenen Punkte. -->
		<Abschnitt titel="Offene Punkte">
			<p class="muted">
				{kopf.open_zahl} offene {kopf.open_zahl === 1 ? 'Punkt' : 'Punkte'}. Anmelden, um sie zu
				sehen: in Microsoft Edge unter {BROWSER_ADRESSE}.
			</p>
		</Abschnitt>
	{/if}

	{#if kopf.daten.length === 0}
		<Abschnitt titel="Daten">
			<p class="muted">Noch keine Dateien in <code>daten/</code>.</p>
		</Abschnitt>
	{/if}

	{#each namen as name (name)}
		<Abschnitt titel={ueberschrift(name)}>
			<StandardDatei {name} />
		</Abschnitt>
	{/each}

	{#if kopf.kopf.links.length > 0}
		<Abschnitt titel="Links">
			<LinksListe links={kopf.kopf.links} />
		</Abschnitt>
	{/if}
</div>

<style>
	/* Nur ein Merkmal: am Aufbau ändert der Ordner nichts. */
	.standardaufbau {
		display: contents;
	}
	.kopf {
		margin: 0 0 0.4em;
	}
	.befunde {
		margin: 0 0 0.4em;
		padding-left: 1.2em;
		overflow-wrap: anywhere;
	}
</style>
