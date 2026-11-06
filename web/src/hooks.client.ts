import type { ClientInit, HandleClientError } from '@sveltejs/kit/hooks';
import { ApiError, istNetzfehler, SATZ_KEINE_VERBINDUNG } from '#lib/api/client.ts';
import { zugang } from '#lib/zugang.ts';
import { istChunkFehler } from '#lib/fassung.ts';
import { fassung } from '#lib/fassung.svelte.ts';

/** Satz der Fehlerseite, während die Seite eine neue Fassung lädt (kurz zu sehen). */
export const SATZ_NEUE_FASSUNG = 'Es gibt eine neue Fassung; die Seite lädt sie gerade.';

/**
 * Etappe 3b: Vor dem ersten Laden einmal fragen, ob eine Anmeldung möglich ist
 * (`zugang.ts`). Ohne Anmeldung (127.0.0.1 ohne Passkey) gehen die
 * geschützten Abfragen dann nicht mehr ins Netz; die Schleusen im Server
 * bleiben, wie sie sind. Höchstens `ZUGANG_FRIST_MS`, dann geht es weiter.
 */
export const init: ClientInit = async () => {
	await zugang.pruefen();
};

/**
 * Fehler des Rust-Servers behalten ihren Code, ihren Satz und ihr festes
 * Wort (`art`), damit die Fehlerseite etwa „Anmeldung nötig“ zeigen kann.
 *
 * Andere Fehler (etwa eine Projektseite, die beim Zeichnen wirft; ihre
 * Fehlergrenze bekommt den Fehler erst nach diesem Haken) behalten ihren
 * Satz statt „Internal Error“, damit der Chat sieht, was los ist. Das
 * Dashboard läuft nur auf diesem PC; der Satz verrät niemandem etwas.
 *
 * Fehlt ein Teil des Baus (nach einer neuen Fassung, 404), lädt die Seite
 * einmal neu (`#lib/fassung.svelte.ts`, mit Schutz vor Schleifen), außer
 * ohne Verbindung (dann „keine Verbindung“, Zustand 1 lädt nie neu). Hat
 * SvelteKit beim Navigieren selbst eine neue Fassung erkannt, navigiert es
 * schon voll zum Ziel und ruft diesen Haken gar nicht.
 */
export const handleError: HandleClientError = ({ kind, error }) => {
	if (kind !== 'unknown') return;
	if (error instanceof ApiError) {
		const { status, message, art } = error;
		return art === null ? { status, message } : { status, message, art };
	}
	// Vor dem Netzfehler: Chrome und Edge sagen hier auch „Failed to fetch …“.
	if (istChunkFehler(error) && fassung.chunkFehler())
		return { status: 503, message: SATZ_NEUE_FASSUNG };
	if (istNetzfehler(error)) return { status: 503, message: SATZ_KEINE_VERBINDUNG };
	console.error(error);
	if (error instanceof Error && error.message !== '')
		return { status: 500, message: error.message };
	return;
};
