<script lang="ts">
	/**
	 * Wacht über die Fassung der App im Tab (Ereignis `fassung`, Etappe 4b;
	 * Vorgaben). Zeigt selbst nichts: Den Zustand zeigt
	 * der Live-Punkt (`LiveBadge.svelte`), nicht die Seite.
	 *
	 * - Das Ereignis `fassung` (freier oder geschützter Kanal, auch ohne
	 *   Anmeldung) nennt den Ordner des neuen Baus; `fassung.ereignis` macht
	 *   die App nur bei einem neueren veraltet.
	 * - Steht der Kanal (nach dem Laden, nach einem Wechsel oder nach einer
	 *   Unterbrechung), fragt die Wache einmal `/api/stand` (frei, mit dem
	 *   Namen der Fassung): Das Ereignis wird nicht wiederholt, eines aus der
	 *   Lücke fehlte sonst.
	 * - Bekam ein Abruf der API keine Antwort (`apiStand`, Zustand 1), obwohl
	 *   der Kanal steht, fragt sie alle paar Sekunden `/api/stand`, bis wieder
	 *   eine Antwort kommt.
	 * - Nur in Zustand 2 (veraltet, verbunden, kein Bau) lädt die Seite nach
	 *   30 s von selbst neu, aber nicht, solange etwas offen ist, das verloren
	 *   ginge (`neuLadenAufschieben`; dann `fassung.aufgehalten`); im
	 *   Hintergrund, sobald der Tab wieder sichtbar ist (`selbstNeuLaden`).
	 *   Vorher fragt sie `/api/stand` (mit Frist `STAND_FRIST_MS`):
	 *   Antwortet der Dienst nicht, gilt Zustand 1, und nichts lädt. Danach
	 *   prüft sie den Sperrgrund noch einmal (er kann während der Frage
	 *   entstanden sein). Reißt die Verbindung ab oder beginnt ein Bau, stoppt
	 *   die Uhr.
	 * - Ohne Anmeldung (kein Ereignis `bau`) holt sie die Projektliste
	 *   (`/api/projects`, darin `bau.zustand`) selbst, mit Frist und direkt in
	 *   `bauLage.liste`, nicht über `invalidate`: Ein `load`, der dabei wirft,
	 *   ließe SvelteKit nach einem Bau voll navigieren (`updated.check`), am
	 *   Sperrgrund vorbei. Sie holt sie beim Ereignis `fassung`,
	 *   alle `NACHFRAGE_MS`, solange laut Liste ein Bau läuft (so endet
	 *   Zustand 3 auch nach rotem Bau), und direkt vor dem Neuladen von selbst
	 *   (mitten in einer Folge von Bauten lädt nichts).
	 *   shortcut: den Beginn eines Baus sieht ein freier Tab erst mit der nächsten Liste (ein Ereignis projekt oder inhalt kommt, wenn überhaupt, vor den 5 s Ruhe des Beobachters), aufrüsten mit einem freien Ja/Nein „Bau läuft“ im Kanal, falls Zustand 3 ohne Anmeldung zu spät erscheint
	 * - Bis dahin macht `beforeNavigate` den nächsten Klick auf einen Link zu
	 *   einer anderen Route zur vollen Navigation (lädt die neue Fassung).
	 * - Nach einem Neuladen durch die Fassung geht der Fokus einmal dorthin
	 *   zurück, wo er stand (`fokusZurueck`), sonst auf `main`.
	 */
	import { beforeNavigate } from '$app/navigation';
	import { onMount } from 'svelte';
	import { standHolen } from '#lib/aktualitaet.svelte.ts';
	import { apiStand } from '#lib/api/antwortet.svelte.ts';
	import { api } from '#lib/api/client.ts';
	import { bauLage } from '#lib/bau.svelte.ts';
	import { fassung } from '#lib/fassung.svelte.ts';
	import {
		NACHFRAGE_MS,
		STAND_FRIST_MS,
		fokusZurueck,
		neuLadenAufschieben,
		selbstNeuLaden,
		umgebung
	} from '#lib/fassung.ts';
	import { freierKanal } from '#lib/freikanal.svelte.ts';

	/** Abruf mit Frist: Hängt der Dienst, gilt nach `STAND_FRIST_MS` keine Antwort. */
	const mitFrist: typeof fetch = (pfad, init) =>
		fetch(pfad, { ...init, cache: 'no-store', signal: AbortSignal.timeout(STAND_FRIST_MS) });
	const holen = () => standHolen(false, mitFrist);
	/** Zählt die Abrufe der Liste: Eine ältere Antwort, die später kommt, gilt nicht. */
	let runde = 0;
	/** Ohne Anmeldung die Projektliste neu (Bau-Stand); angemeldet kommt `bau` live. Wirft nie. */
	async function listeNeu(): Promise<void> {
		if (freierKanal.geschuetztOk) return;
		const meine = ++runde;
		try {
			const l = await api.projects(mitFrist);
			if (meine === runde) bauLage.liste(l.projects);
		} catch {
			// Keine Antwort: der alte Stand bleibt (`apiStand` meldet Zustand 1).
		}
	}

	onMount(() => {
		fokusZurueck(document, umgebung.speicher());
		return freierKanal.onFassung((f) => {
			fassung.ereignis(f.ordner);
			void listeNeu();
		});
	});

	let vorher = freierKanal.kanal;
	$effect(() => {
		const jetzt = freierKanal.kanal;
		if (vorher !== 'offen' && jetzt === 'offen')
			void holen().then((b) => {
				if (b !== null) fassung.ereignis(b.stand.fassung);
			});
		vorher = jetzt;
	});

	$effect(() => {
		if (apiStand.antwortet || freierKanal.kanal !== 'offen') return;
		const uhr = setInterval(() => void holen(), NACHFRAGE_MS);
		return () => {
			clearInterval(uhr);
		};
	});

	// Zustand 3 ohne Anmeldung: die Liste nachholen, bis sie keinen laufenden Bau mehr nennt.
	$effect(() => {
		if (!fassung.baut || freierKanal.geschuetztOk) return;
		const uhr = setInterval(() => void listeNeu(), NACHFRAGE_MS);
		return () => {
			clearInterval(uhr);
		};
	});

	$effect(() => {
		if (fassung.lage !== 'veraltet') return;
		const aus = selbstNeuLaden({
			aufschieben: () => neuLadenAufschieben(document),
			verborgen: () => document.visibilityState === 'hidden',
			beiSichtbar: (f) => {
				const sichtbar = (): void => {
					if (document.visibilityState === 'visible') f();
				};
				document.addEventListener('visibilitychange', sichtbar);
				return () => {
					document.removeEventListener('visibilitychange', sichtbar);
				};
			},
			// Erst fragen, ob der Dienst antwortet: Ohne Antwort (Zustand 1) lüde die Seite
			// nur die Fehlerseite des Browsers, auch wenn der Kanal noch offen scheint.
			// Ohne Anmeldung auch die Liste: Steht dort ein neuer Bau an, gilt 3 statt 2.
			// shortcut: Zustand 1 bei offenem Kanal und totem Netz erst nach bis zu 30 s (hier); aufrüsten, wenn das stört (öfter nachfragen)
			laden: async () => {
				await Promise.all([holen(), listeNeu()]);
				// Während der Frage kann etwas aufgegangen sein (Feld, Dialog, Freigabe).
				if (neuLadenAufschieben(document)) return false;
				return fassung.lage === 'veraltet' ? fassung.selbstLaden() : true;
			},
			aufgehalten: (ja) => {
				fassung.aufgehalten = ja;
			}
		});
		return () => {
			aus();
			fassung.aufgehalten = false;
		};
	});

	beforeNavigate((nav) => {
		fassung.vorNavigation(nav);
	});
</script>
