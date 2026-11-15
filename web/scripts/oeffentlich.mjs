// Schritt E2b: die öffentliche Menge des Baus. Ohne Anmeldung liefert der
// Dienst unter /_app/ nur Dateien aus `_app/oeffentlich.json` (src/app.rs,
// `WebOrt::in_oeffentlicher_liste`). Dieses Vite-Plugin schreibt die Liste im
// Client-Bau: alle Ausgabedateien, die von den Einstiegen (entry/*, nodes/*)
// erreichbar sind (statische und dynamische Importe, CSS, Assets), ohne die
// Kante in die Bauliste (projekte.generated.ts). Dazu version.json und die
// Liste selbst.
//
// Die Schranke: Die geschützte Menge kommt aus dem Modulgraphen, nicht aus
// Ordnerlisten (ein Bau von Hand hat etwa kein DASHBOARD_PROJECTS_DIR): was
// die Bauliste lädt (immer), und alles, was nur von dort erreichbar ist.
// Virtuelle Module (`\0…`, `virtual:…`) zählen nie als geschützt. Der Bau
// bricht ab, wenn ein Einstieg eine Projektseite an der Bauliste vorbei
// erreicht, eine öffentliche Datei ein Modul dieser Menge enthält, ein
// öffentliches Asset aus einem solchen Modul stammt, die Bauliste selbst
// öffentlich wäre oder ihr Teil Code der Hülle trägt. Meldungen nennen nur
// Dateinamen des Baus und Zahlen, nie einen Pfad eines Projekts.
//
// Ohne Abhängigkeiten außer node:path.
import { isAbsolute, resolve } from 'node:path';

/** Der Name der Liste unter dem Ordner der App (`_app`). */
export const LISTE = 'oeffentlich.json';

/**
 * Der Pfad eines Moduls ohne Abfrage (`?svelte&type=style…`), mit `/`;
 * `null` für virtuelle Module (`\0…`, `virtual:…`, `<…>`), die nie als
 * Projektmodul zählen. Unter Windows ohne Unterschied von Groß und Klein.
 * @param {string} id
 * @param {string} [wurzel] für relative Namen (Assets)
 * @returns {string | null}
 */
export function modulPfad(id, wurzel) {
	if (id === '' || id.startsWith('\0') || id.startsWith('virtual:') || id.startsWith('<')) {
		return null;
	}
	let pfad = id.replace(/[?#].*$/s, '');
	if (pfad === '') return null;
	if (wurzel !== undefined && !isAbsolute(pfad)) pfad = resolve(wurzel, pfad);
	pfad = pfad.replaceAll('\\', '/');
	return process.platform === 'win32' ? pfad.toLowerCase() : pfad;
}

/**
 * Alle Ids, die von `start` über `kanten` erreichbar sind, ohne `ohne`.
 * @param {Iterable<string>} start
 * @param {(id: string) => Iterable<string>} kanten
 * @param {string} ohne
 * @returns {Set<string>}
 */
function erreichbar(start, kanten, ohne) {
	const da = new Set();
	const offen = [...start].filter((id) => id !== ohne);
	while (offen.length > 0) {
		const id = /** @type {string} */ (offen.pop());
		if (da.has(id)) continue;
		da.add(id);
		for (const n of kanten(id)) if (n !== ohne && !da.has(n)) offen.push(n);
	}
	return da;
}

/**
 * @typedef {{
 *   type: 'chunk', fileName: string, isEntry: boolean, moduleIds: string[],
 *   imports: string[], dynamicImports: string[],
 *   viteMetadata?: { importedCss: Set<string>, importedAssets: Set<string> }
 * }} Teil
 * @typedef {{ type: 'asset', fileName: string, originalFileNames?: string[] }} Datei
 * @typedef {{ importedIds: readonly string[], dynamicallyImportedIds: readonly string[], isEntry: boolean }} Info
 */

/**
 * Die öffentliche Menge und die Befunde der Schranke (ohne Vite, für Tests).
 * @param {{
 *   bundle: Record<string, Teil | Datei>,
 *   ids: string[],
 *   info: (id: string) => Info | null,
 *   tabelle: string,
 *   wurzel: string,
 *   appDir: string
 * }} a
 * @returns {{ dateien: string[], fehler: string[] }}
 */
export function oeffentlicheMenge({ bundle, ids, info, tabelle, wurzel, appDir }) {
	const fehler = [];
	const tabSchluessel = modulPfad(tabelle);
	const tabId = ids.find((id) => modulPfad(id) === tabSchluessel);
	if (tabId === undefined) {
		return { dateien: [], fehler: ['Die Bauliste (projekte.generated.ts) fehlt im Modulgraphen.'] };
	}
	const kanten = (/** @type {string} */ id) => {
		const m = info(id);
		return m ? [...m.importedIds, ...m.dynamicallyImportedIds] : [];
	};
	// Module: öffentlich ist, was die Einstiege ohne die Bauliste erreichen.
	const einstiege = ids.filter((id) => info(id)?.isEntry === true);
	const oeffentlichePfade = new Set(
		[...erreichbar(einstiege, kanten, tabId)].map((id) => modulPfad(id)).filter((p) => p !== null)
	);
	const tab = info(tabId);
	const direkt = tab ? [...tab.dynamicallyImportedIds, ...tab.importedIds] : [];
	const vonTabelle = erreichbar(direkt, kanten, tabId);
	/** @type {Set<string>} */
	const geschuetzt = new Set();
	for (const id of vonTabelle) {
		const p = modulPfad(id);
		if (p !== null && !oeffentlichePfade.has(p)) geschuetzt.add(p);
	}
	// Die direkten Ziele der Bauliste (Seite.svelte, seiten/*.svelte) sind immer
	// geschützt; erreicht ein Einstieg eins, wäre die Projektseite still öffentlich.
	const seiten = [...new Set(direkt.map((id) => modulPfad(id)).filter((p) => p !== null))];
	for (const p of seiten) geschuetzt.add(p);
	const erreicht = seiten.filter((p) => oeffentlichePfade.has(p)).length;
	if (erreicht > 0) {
		fehler.push(
			`Die Hülle erreicht ${erreicht} eigene Projektseiten ohne die Bauliste (sie dürfen nur über die Bauliste geladen werden).`
		);
	}
	const istGeschuetzt = (/** @type {string} */ id) => {
		const p = modulPfad(id);
		return p !== null && geschuetzt.has(p);
	};

	// Dateien: von den Einstiegen über Importe, CSS und Assets, ohne die Teile mit der Bauliste.
	const teile = /** @type {Teil[]} */ (Object.values(bundle).filter((b) => b.type === 'chunk'));
	const tabTeile = new Set(
		teile.filter((t) => t.moduleIds.some((id) => id === tabId)).map((t) => t.fileName)
	);
	for (const t of teile.filter((t) => tabTeile.has(t.fileName))) {
		const fremd = t.moduleIds.filter(
			(id) => id !== tabId && modulPfad(id) !== null && !istGeschuetzt(id)
		);
		if (fremd.length > 0) {
			fehler.push(
				`${t.fileName}: Der Teil mit der Bauliste enthält ${fremd.length} Module der Hülle (die Bauliste muss ein eigener Teil sein).`
			);
		}
	}
	/** @type {Set<string>} */
	const oeffentlich = new Set();
	const offen = teile.filter((t) => t.isEntry).map((t) => t.fileName);
	while (offen.length > 0) {
		const name = /** @type {string} */ (offen.pop());
		if (oeffentlich.has(name)) continue;
		oeffentlich.add(name);
		const b = bundle[name];
		if (b?.type !== 'chunk') continue;
		const weiter = [
			...b.imports,
			...b.dynamicImports,
			...(b.viteMetadata?.importedCss ?? []),
			...(b.viteMetadata?.importedAssets ?? [])
		];
		for (const n of weiter) if (!tabTeile.has(n) && !oeffentlich.has(n)) offen.push(n);
	}
	for (const name of tabTeile) {
		if (oeffentlich.has(name))
			fehler.push(`${name}: Die Bauliste stünde in der öffentlichen Menge.`);
	}
	for (const name of oeffentlich) {
		const b = bundle[name];
		if (b?.type === 'chunk') {
			const n = b.moduleIds.filter(istGeschuetzt).length;
			if (n > 0 && !tabTeile.has(name)) {
				fehler.push(`${name}: Die öffentliche Datei enthält ${n} Module eigener Projektseiten.`);
			}
		} else if (b?.type === 'asset') {
			const herkunft = (b.originalFileNames ?? []).map((f) => modulPfad(f, wurzel));
			if (herkunft.some((p) => p !== null && geschuetzt.has(p))) {
				fehler.push(`${name}: Das öffentliche Asset stammt aus einer eigenen Projektseite.`);
			}
		}
	}
	oeffentlich.add(`${appDir}/version.json`);
	oeffentlich.add(`${appDir}/${LISTE}`);
	return { dateien: [...oeffentlich].sort(), fehler };
}

/**
 * Das Plugin; nur im Client-Bau.
 * @param {{ tabelle: string, appDir?: string }} o `tabelle`: absoluter Pfad von projekte.generated.ts
 * @returns {import('vite').Plugin}
 */
export function oeffentlicheListe({ tabelle, appDir = '_app' }) {
	/** @type {string} */
	let wurzel = process.cwd();
	return {
		name: 'dashboard:oeffentlich',
		apply: 'build',
		enforce: 'post',
		applyToEnvironment: (umgebung) => umgebung.name === 'client',
		configResolved(c) {
			wurzel = c.root;
		},
		generateBundle(_optionen, bundle) {
			const { dateien, fehler } = oeffentlicheMenge({
				bundle: /** @type {Record<string, Teil | Datei>} */ (/** @type {unknown} */ (bundle)),
				ids: [...this.getModuleIds()],
				info: (id) => this.getModuleInfo(id),
				tabelle,
				wurzel,
				appDir
			});
			if (fehler.length > 0) {
				this.error(`Öffentliche Menge des Baus (Schritt E2b): ${fehler.join(' ')}`);
			}
			this.emitFile({
				type: 'asset',
				fileName: `${appDir}/${LISTE}`,
				source: `${JSON.stringify(dateien, null, '\t')}\n`
			});
		}
	};
}
