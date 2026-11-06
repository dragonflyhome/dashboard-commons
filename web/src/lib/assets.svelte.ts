/**
 * Live-Lücken, Punkt 5: Version der Dateien in `assets/` je Projekt.
 *
 * Ändert sich ein Bild oder eine Vorschau in `projects/<slug>/assets/`,
 * schickt der Server `projekt` mit `assets: true` (beide Kanäle; die Dateien
 * sind frei). Die Seite merkt sich die Version dazu und hängt sie als
 * `?v=<version>` an die Adresse (`mitVersion`): Die Adresse ist neu, also
 * holt der Browser die Datei neu, ohne dass die Seite neu lädt. Bis zur
 * ersten Änderung bleibt die Adresse ohne Version.
 *
 * Je Datei (Kleinigkeit aus der Abnahme live-luecken): Auf dem geschützten
 * Kanal nennt `projekt` dazu die geänderten Dateien (`dateien`). Ein Bild
 * bekommt dann nur eine neue Version, wenn seine Datei dabei war; die anderen
 * Bilder des Projekts bleiben im Browser-Cache. Ohne Namen (freier Kanal,
 * sehr viele Dateien) gilt die Version für alle Dateien des Projekts. Die
 * Vorschau fragt ohne Dateinamen und bekommt die neueste Version des
 * Projekts: Sie kann andere Dateien aus `assets/` einbinden.
 */
import { SvelteMap } from 'svelte/reactivity';
import { freierKanal } from './freikanal.svelte.ts';
import { live, type InhaltAenderung } from './live.svelte.ts';

export class AssetStand {
	/** Je Projekt: Version einer Änderung ohne Dateinamen (gilt für alle Dateien). */
	#projektweit = new SvelteMap<string, number>();
	/** Je `<slug>/<datei>`: Version der letzten Änderung genau dieser Datei. */
	#dateien = new SvelteMap<string, number>();
	/** Je Projekt: die neueste Version irgendeiner Änderung in `assets/`. */
	#neueste = new SvelteMap<string, number>();

	/**
	 * Die Version für eine Datei dieses Projekts (`datei` ohne Pfad), sonst
	 * `null`. Ohne `datei`: die neueste Version irgendeiner Datei des Projekts.
	 */
	version(slug: string, datei?: string): number | null {
		if (datei === undefined) return this.#neueste.get(slug) ?? null;
		const p = this.#projektweit.get(slug);
		const d = this.#dateien.get(`${slug}/${datei}`);
		if (p === undefined) return d ?? null;
		if (d === undefined) return p;
		return Math.max(p, d);
	}

	/** Nimmt eine Änderung auf (nur die mit `assets`). */
	aufnehmen(a: InhaltAenderung): void {
		for (const slug of a.assets ?? []) {
			this.#neueste.set(slug, Math.max(this.#neueste.get(slug) ?? 0, a.version));
			if (a.assetDateien === undefined) {
				this.#projektweit.set(slug, a.version);
			} else {
				for (const datei of a.assetDateien) this.#dateien.set(`${slug}/${datei}`, a.version);
			}
		}
	}

	/** Hört auf beide Kanäle; gibt die Abmeldung zurück. */
	start(): () => void {
		const offLive = live.onInhalt((a) => {
			this.aufnehmen(a);
		});
		const offFrei = freierKanal.onInhalt((a) => {
			this.aufnehmen(a);
		});
		return () => {
			offLive();
			offFrei();
		};
	}

	/** Für Tests. */
	vergessen(): void {
		this.#projektweit.clear();
		this.#dateien.clear();
		this.#neueste.clear();
	}
}

/** Die Adresse mit `v=<version>`; ohne Version unverändert. */
export function mitVersion(adresse: string, version: number | null): string {
	if (version === null) return adresse;
	return `${adresse}${adresse.includes('?') ? '&' : '?'}v=${version}`;
}

export const assetStand = new AssetStand();
