<script lang="ts">
	import '../app.css';
	import { afterNavigate, invalidate, refreshAll } from '$app/navigation';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import { bereichBetroffen, betroffen, live, vorgangBetroffen } from '#lib/live.svelte.ts';
	import { assetStand } from '#lib/assets.svelte.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';
	import { aufgaben } from '#lib/aufgaben.svelte.ts';
	import { anstehend } from '#lib/anstehend.svelte.ts';
	import { bauLage } from '#lib/bau.svelte.ts';
	import { passkeyAntraege } from '#lib/passkey.svelte.ts';
	import { kopfleiste } from '#lib/kopfleiste.ts';
	import { aktualitaet } from '#lib/aktualitaet.svelte.ts';
	import { pcMinute } from '#lib/pcminute.svelte.ts';
	import { Schublade } from '$katalog';
	import Brotkrumen from '#lib/components/kopf/Brotkrumen.svelte';
	import Seitenleiste from '#lib/components/kopf/Seitenleiste.svelte';
	import PcSymbol from '#lib/components/kopf/PcSymbol.svelte';
	import Glocke from '#lib/components/kopf/Glocke.svelte';
	import LiveBadge from '#lib/components/LiveBadge.svelte';
	import Schloss from '#lib/components/Schloss.svelte';
	import ThemaSchalter from '#lib/components/ThemaSchalter.svelte';
	import FassungWache from '#lib/components/FassungWache.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();

	// Kopfleiste, Fassung 3: Brotkrumen und Markierung der Leiste aus Route und den schon geladenen Daten.
	const kopf = $derived(kopfleiste(page.route.id, page.params, page.data));

	/** Die Schublade (unter 1024 px); schließt bei jedem Seitenwechsel. */
	let schublade = $state(false);
	afterNavigate(() => {
		schublade = false;
	});

	// Etappe 4c: Jede neue Projektliste ist der Bau-Stand des Servers (frei).
	// shortcut: ohne den Runden-Schutz der FassungWache kann ein späteres load eine neuere Liste überschreiben, Zustand 3 flackert einige Sekunden; aufrüsten: Zähler in BauLage
	$effect(() => {
		bauLage.liste(data.leisteProjekte);
	});

	// Das PC-Symbol: die Werte des einen Kanals (geschützt oder frei), doppelte zählen nicht.
	$effect(() => {
		pcMinute.aufnehmen(live.pc ?? freierKanal.pc);
	});

	/** Der Klick auf das Blatt: alles nachziehen, was die Seite zeigt (ohne neue Browserseite). */
	function nachziehen(): Promise<unknown> {
		return Promise.all([
			// SvelteKit 3: refreshAll ist das frühere invalidateAll (alle load neu).
			refreshAll(),
			anstehend.laden(),
			aufgaben.zaehlen(),
			aufgaben.benutzt ? aufgaben.laden() : undefined,
			passkeyAntraege.zaehlen()
		]);
	}

	onMount(() => {
		// Kopfleiste, Fassung 2: genau ein Kanal je Seite. Angemeldet der geschützte
		// (live.svelte.ts), sonst der freie; freikanal.svelte.ts wählt und wechselt.
		freierKanal.start();
		// Live-Kanal der offenen Aufgaben; die Liste laden die Kästen selbst (Start, Projekte).
		const offAufgaben = aufgaben.start();
		// „Anstehend“ einmal für alle: Zahl an „Anstehend“, Glocke, Zähler „wartet“ der Leiste.
		const offAnstehend = anstehend.start();
		// Offene Passkey-Anträge an „Schlüssel“ (Etappe 7c); ohne Anmeldung 0.
		const offAntraege = passkeyAntraege.start();
		// Etappe 4c: Bau-Stand der Seiten (Sätze nur angemeldet; Zustand 3 am Live-Punkt).
		const offBau = bauLage.start();
		// Ein Bau ist fertig (grün oder rot): die Projektliste mit dem Bau-Stand neu.
		const offBauListe = live.onBau((b) => {
			if (b.zustand !== 'laeuft') void invalidate((u) => u.href === 'api:projects');
		});
		// Kopfleiste, Fassung 3: mitzählen, was der Kanal bringt (Aktualitätsprüfung im Blatt).
		const offAktualitaet = aktualitaet.start();
		// Etappe 2d: Nur was sich geändert hat, lädt neu (ein Projekt: Liste und
		// seine Seite; data/pc.toml: die PC-Werte), ohne Neuladen der Seite.
		// Viele auf einmal kommen als ein Ereignis, also auch nur ein Neuladen.
		const off = live.onInhalt((a) => {
			void invalidate(betroffen(a));
		});
		// Ohne geschützten Kanal (nicht angemeldet) kommen schon freie Änderungen über den freien.
		const offFrei = freierKanal.onInhalt((a) => {
			if (!live.connected) void invalidate(betroffen(a));
		});
		// Vorgänge kommen nicht als Dateiänderung, sondern als eigenes Ereignis.
		// Live-Lücken: gezielt die Übersicht der Vorgänge statt refreshAll; was ein
		// Vorgang sonst ändert, kommt als `speicher` mit Bereich `vorgaenge`.
		const offVorgang = live.onVorgang(() => {
			void invalidate(vorgangBetroffen());
		});
		// Live-Lücken: Datenspeicher, Benutzer-Daten und docs/, je Bereich gezielt.
		const offBereich = live.onBereich((b) => {
			void invalidate(bereichBetroffen(b));
		});
		// Geänderte Bilder und Vorschauen: neue Version in der Adresse (beide Kanäle).
		const offAssets = assetStand.start();
		// Verpasste Ereignisse (Lücke in den Nummern, Verbindung war weg): alles neu laden.
		const offNachholen = live.onNachholen(() => {
			void refreshAll();
		});
		// Wird das Fenster breit (ab 1024 px steht die Leiste fest), ist die Schublade überflüssig.
		// Ihr Knopf ist dann ausgeblendet; der Fokus geht auf den Inhalt (`ersatz` oben).
		const breit = window.matchMedia('(min-width: 64rem)');
		const beiBreite = (e: MediaQueryListEvent): void => {
			if (e.matches) schublade = false;
		};
		breit.addEventListener('change', beiBreite);
		return () => {
			off();
			offFrei();
			freierKanal.stop();
			offVorgang();
			offBereich();
			offAssets();
			offNachholen();
			offAufgaben();
			offAnstehend();
			offAntraege();
			offBau();
			offBauListe();
			offAktualitaet();
			breit.removeEventListener('change', beiBreite);
		};
	});
</script>

{#snippet leiste(ongewaehlt?: () => void)}
	<Seitenleiste
		projekte={data.leisteProjekte}
		gruppen={data.projektListe?.gruppen ?? []}
		markiert={kopf.markiert}
		daten={page.data}
		wartet={anstehend.wartet}
		bau={bauLage.live}
		jetztZahl={anstehend.jetzt}
		antragZahl={passkeyAntraege.offen}
		{ongewaehlt}
	/>
{/snippet}

<header class="top">
	<!-- Unter 1024 px: die Seitenleiste als Schublade hinter einem Knopf (44 px). -->
	<div class="schublade">
		<Schublade
			beschriftung="Seiten und Projekte"
			titel="Seiten und Projekte"
			bind:offen={schublade}
			ersatz={() => document.getElementById('inhalt')}
		>
			{@render leiste(() => (schublade = false))}
		</Schublade>
	</div>
	<!-- Links: Brotkrumen; das Blatt prüft, ob alles aktuell ist, und scrollt nach oben. -->
	<div class="links">
		<Brotkrumen
			{kopf}
			laeuft={aktualitaet.laeuft}
			ergebnis={aktualitaet.ergebnis}
			onblatt={() => aktualitaet.pruefen(nachziehen)}
		/>
	</div>
	<!-- Rechts, in dieser Reihenfolge: PC, Glocke, Live-Punkt, Schloss. -->
	<div class="rechts">
		<PcSymbol />
		<Glocke />
		<LiveBadge />
		<Schloss />
	</div>
</header>

<div class="rahmen">
	<!-- Ab 1024 px steht die Leiste fest links. -->
	<div class="leiste-fest">
		{@render leiste()}
	</div>
	<!-- tabindex -1: Fokusziel (etwa nach „gelesen“, wenn nichts mehr dasteht). -->
	<main id="inhalt" tabindex="-1">
		{@render children()}
	</main>
</div>

<footer class="muted small">
	<!-- Etappe 3b: Thema je Browser (System, Hell, Dunkel), hier unten statt in der Kopfleiste. -->
	<ThemaSchalter />
	<p>Dein Dashboard läuft nur hier auf diesem PC.</p>
</footer>

<!-- Neue Fassung nach einem Bau: Hinweis am Live-Punkt, lädt nach 30 s von selbst (Etappe 4b, Wunsch). -->
<FassungWache />

<style>
	.top {
		position: sticky;
		top: 0;
		z-index: 10;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		min-height: 4rem;
		padding: 0.625rem clamp(0.875rem, 3vw, 2rem);
		background: color-mix(in srgb, var(--card) 85%, transparent);
		backdrop-filter: blur(10px);
		border-bottom: 1px solid var(--line);
	}
	.schublade {
		display: flex;
		flex: none;
	}
	.links {
		flex: 1 1 auto;
		min-width: 0;
	}
	/* Rechts: PC, Glocke, Live-Punkt und Schloss, nie umbrechend. */
	.rechts {
		display: flex;
		flex: none;
		align-items: center;
		gap: 0.25rem;
		margin-left: auto;
	}
	.rahmen {
		display: block;
	}
	.leiste-fest {
		display: none;
	}
	main:focus {
		outline: none;
	}
	main {
		min-width: 0;
		max-width: 1180px;
		margin: 0 auto;
		padding: 22px clamp(14px, 3vw, 32px) 40px;
	}
	/* Ab 1024 px: die Leiste fest links, kein Knopf für die Schublade. */
	@media (min-width: 64rem) {
		.schublade {
			display: none;
		}
		.rahmen {
			display: grid;
			grid-template-columns: 16rem minmax(0, 1fr);
			align-items: start;
		}
		.leiste-fest {
			position: sticky;
			top: 4rem;
			display: block;
			max-height: calc(100vh - 4rem);
			overflow-y: auto;
			overscroll-behavior: contain;
			border-right: 1px solid var(--line);
		}
		main {
			width: 100%;
		}
	}
	/* Handy: zwei Zeilen. Oben Knopf und rechts die Zeichen, darunter die
	   Brotkrumen über die ganze Breite; nicht klebend, damit der Inhalt sichtbar
	   bleibt. */
	@media (max-width: 37.5rem) {
		.top {
			position: static;
			flex-wrap: wrap;
			gap: 0 0.25rem;
			padding: 0.375rem 0.75rem 0.25rem;
		}
		.links {
			order: 3;
			flex: 1 0 100%;
		}
	}
	footer {
		display: grid;
		justify-items: center;
		gap: 0.5rem;
		text-align: center;
		padding: 20px;
	}
	footer p {
		margin: 0;
	}
</style>
