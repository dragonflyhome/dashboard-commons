// Schritt E2b, Dauerprobe auf der Platte: Nach jedem Bau (package.json, „build“)
// durchsucht sie jede öffentliche Datei (außerhalb von _app/ alle, unter _app/
// die aus _app/oeffentlich.json) nach allen Slugs der Bauliste und nach den
// Texten der eigenen Projektseiten, in Fenstern von vier Wörtern.
//
// Frei sind nur Begriffe, die auch in den Quellen der Hülle stehen (web/src
// ohne Bauliste), etwa ein Slug, der zugleich ein Weg ist: Sie stehen ohnehin
// im Code-Repo, und ihr Vorkommen verrät nichts. Das wird bei jedem Lauf aus
// der Hülle abgeleitet und nirgends gespeichert; eine gespeicherte Liste mit
// Abdrücken ließe sich gegen die Wörter der Hülle zurückrechnen und verriete,
// welche Wörter Projekte sind.
//
// Gegen Blindheit (Stufe 2, auch im Bau ein Fehler): Findet die Probe weniger
// als 80 % der Seitentexte in den geschützten Dateien oder gar keinen
// prüfbaren Text, fehlt eine Seitendatei der Bauliste oder steht dort ein
// Eintrag, den sie nicht lesen kann, kann sie nicht prüfen. Die 80 % gelten
// zusätzlich je Seite eines Slugs, der zugleich ein Wort der Hülle ist: Den
// Slug zählt sie nicht, ein Leck dieser Seite zeigt sich nur über ihre Texte.
// Darum sucht sie dort auch kurze Stücke (`kurz`: ab zwei Wörtern, im Text
// auch ein Wort ab zehn Zeichen). Eine solche Seite ganz ohne solche Texte
// zählt als textlos; dort gibt es nichts zu suchen, ein Leck fängt nur die
// Schranke im Bau. Nennt die Bauliste keine Seite, ist das nur grün, wenn sie
// die Form des Erzeugers ohne Einträge hat und der Bau höchstens eine
// geschützte Datei trägt (die leere Tabelle); sonst passen Bau und Liste nicht.
// Ausgegeben werden nur Zahlen und Dateinamen des Baus, nie ein Slug oder ein Satz.
//
// Alle Texte einer Seite nimmt sie aus dem Syntaxbaum (`baumTexte`, mit
// svelte/compiler): Text im Markup, Attributwerte (in "…" und '…'),
// Zeichenketten in Ausdrücken des Markups (`{c ? '…' : '…'}`, `label={"…"}`,
// `{@html '…'}`) und im <script>. Kommentare und Typen (`type Art = 'a b'`,
// `as '…'`, Interfaces, Schlüssel in Typen) zählen nicht, ein Regex-Literal
// ist weder Kommentar noch Zeichenkette, verschachtelte Template-Literale
// zählen je Stück. Kommentare und Typen fehlen im Bau und machten sonst eine
// Seite blind.
//
// Grenzen: Texte kommen nur aus den Dateien, die die Bauliste nennt (nicht aus
// Modulen, die eine Seite einbindet); einzelne Wörter unter zehn Zeichen sucht
// sie nie; huellenText liest alle Dateien unter web/src, auch nicht
// eingecheckte; virtuelle Module zählen im Bau nie als geschützt
// (scripts/oeffentlich.mjs). Zeichenketten in totem Code (auch in einem
// Zweig, der nie gezeichnet wird) zählen weiter als Seitentext, stehen aber
// nicht im Bau; auf einer Seite eines Slugs, der ein Wort der Hülle ist, kann
// das die Seite blind machen (dann den Code löschen). Ein Text, den Svelte
// anders schreibt als die Quelle (Entitäten außer &amp; &lt; &gt; &quot; &#39;
// &nbsp;), wird nicht gefunden und zählt als ungesehen. Lässt sich eine
// Seitendatei nicht lesen, ist die Probe blind (Stufe 2).
//
// Aufruf (im Ordner web/): node scripts/oeffentlich-probe.mjs [bauordner]
// Ohne Bauordner: DASHBOARD_BUILD_ZIEL (wie vite.config.ts), sonst build.
// Rückgabe 0: grün, auch ohne Projektseiten; 1: Funde, dann heißt die Liste
// _app/oeffentlich.json.fund (ließ sie sich nicht umbenennen, steht dort eine
// Marke, damit die Skripte den Fund erkennen), Fund-Dateien außerhalb von _app/ liegen unter
// _app/fund/, und der Ordner gibt ohne Anmeldung nichts davon heraus (fail
// closed; lässt sich etwas nicht umbenennen, wird es gelöscht, es ist
// Bau-Ausgabe); 2: kann nicht prüfen (keine oder kaputte Liste, blind), dann
// steht die Marke _app/oeffentlich.blind im Bau (für scripts\Dienst.ps1 und
// Start-Dashboard.ps1: kein Fehler eines Projekts). Jeder Lauf entfernt eine
// alte Marke zuerst.
import {
	existsSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	renameSync,
	rmSync,
	statSync,
	writeFileSync
} from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'svelte/compiler';

const WEB = resolve(dirname(fileURLToPath(import.meta.url)), '..');
/** Endungen, die als Text gelesen werden; Bilder und Schriften tragen keinen Satz. */
const TEXT = /\.(js|mjs|css|html|json|txt|svg|webmanifest|xml)$/i;
/** Ein Fenster: so viele Wörter, mindestens so viele Zeichen. */
const FENSTER = 4;
const MIN_ZEICHEN = 20;
/** Kürzere Texte zählen ganz, ab drei Wörtern und so vielen Zeichen. */
const MIN_WOERTER = 3;
const MIN_KURZ = 15;
/** So viele Seitentexte muss die Probe in den geschützten Dateien finden. */
const MIN_GESEHEN = 0.8;
/** Kurze Stücke (Seiten eines Slugs, der ein Wort der Hülle ist): ein Wort im Text ab so vielen Zeichen. */
const MIN_WORT = 10;
/** Die Bauliste ohne Einträge, wie `scanner::bauliste_text` sie schreibt (beide Tabellen). */
const LEERE_TABELLE =
	/^export const projekt(?:Seiten|Unterseiten): Record<string, Laden> = \{\};$/gm;

/**
 * Die Einträge der Bauliste (projekte.generated.ts), je Slug und Seitendatei;
 * `unlesbar`: Einträge (`() => import(`), die das Muster nicht trifft.
 * @param {string} text
 * @returns {{ slugs: string[], dateien: string[], seiten: { slug: string, datei: string }[], unlesbar: number }}
 */
export function baulisteLesen(text) {
	const seiten = [
		...text.matchAll(
			/^\s*'?([a-z0-9][a-z0-9-]*)(?:\/[a-z0-9-]+)?'?\s*:\s*\(\)\s*=>\s*import\('([^']+)'\)/gm
		)
	].map((m) => ({ slug: m[1] ?? '', datei: m[2] ?? '' }));
	return {
		slugs: [...new Set(seiten.map((s) => s.slug))],
		dateien: [...new Set(seiten.map((s) => s.datei))],
		seiten,
		unlesbar: [...text.matchAll(/\(\)\s*=>\s*import\(/g)].length - seiten.length
	};
}

/**
 * Die Texte einer Svelte-Datei aus dem Syntaxbaum. `text`: Text im Markup
 * zwischen Tags und Ausdrücken. `sonst`: Text in Attributwerten (auch in '…'),
 * Zeichenketten (Literale und Stücke von Template-Literalen) in Ausdrücken des
 * Markups (`{c ? '…' : '…'}`, `label={"…"}`, `{@html '…'}`) und in den Skripten
 * (Instanz und Modul). Ohne Kommentare und ohne Typen: Von einem TS-Knoten zählt
 * nur sein Ausdruck (`x as 'T'`: x), Typen, Interfaces und Typ-Schlüssel nie.
 * Wirft, wenn sich die Datei nicht lesen lässt.
 * @param {string} quelle
 * @returns {{ text: string[], sonst: string[] }}
 */
export function baumTexte(quelle) {
	const ast = parse(quelle, { modern: true });
	/** @type {string[]} */
	const text = [];
	/** @type {string[]} */
	const sonst = [];
	const gesehen = new Set();
	const lauf = (/** @type {unknown} */ n, /** @type {boolean} */ imAttribut) => {
		if (Array.isArray(n)) return n.forEach((x) => lauf(x, imAttribut));
		if (!n || typeof n !== 'object' || gesehen.has(n)) return;
		gesehen.add(n);
		const k = /** @type {Record<string, any>} */ (n);
		if (k.type === 'Comment') return;
		if (typeof k.type === 'string' && k.type.startsWith('TS'))
			return lauf(k.expression, imAttribut);
		if (k.type === 'Text') (imAttribut ? sonst : text).push(k.data);
		if (k.type === 'Literal' && typeof k.value === 'string') sonst.push(k.value);
		if (k.type === 'TemplateElement') sonst.push(k.value.cooked ?? k.value.raw);
		const attr = imAttribut || k.type === 'Attribute';
		for (const [name, v] of Object.entries(k))
			if (name !== 'leadingComments' && name !== 'trailingComments') lauf(v, attr);
	};
	lauf(ast.fragment, false);
	lauf(ast.instance?.content, false);
	lauf(ast.module?.content, false);
	return { text, sonst };
}

/**
 * Text, wie er im Bau und in den Quellen verglichen wird: Zeichen-Entitäten
 * aufgelöst, Leerraum (auch `\n` und `\t` in Zeichenketten) zu einem Leerzeichen.
 * @param {string} text
 */
export function eben(text) {
	return text
		.replace(
			/&(amp|lt|gt|quot|#39|nbsp);/g,
			(_, e) =>
				({ amp: '&', lt: '<', gt: '>', quot: '"', '#39': "'", nbsp: ' ' })[/** @type {'amp'} */ (e)]
		)
		.replace(/(?:\s|\\[ntr])+/g, ' ');
}

/**
 * Die Texte einer Svelte-Datei (`baumTexte`), je mit mindestens drei Wörtern.
 * `kurz`: auch kürzere Stücke ab zwei Wörtern, im Text auch ein Wort ab
 * `MIN_WORT` Zeichen (in Attributen und Zeichenketten nicht: dort stehen
 * Klassen und Importpfade).
 * @param {string} quelle
 * @param {boolean} [kurz]
 * @returns {string[]}
 */
export function satzfolgen(quelle, kurz = false) {
	const { text, sonst } = baumTexte(quelle);
	const aus = new Set();
	for (const [stuecke, imText] of /** @type {const} */ ([
		[text, true],
		[sonst, false]
	])) {
		for (const stueck of stuecke) {
			const t = eben(stueck).trim();
			const w = t === '' ? 0 : t.split(' ').length;
			const satz = t.length >= MIN_KURZ && w >= MIN_WOERTER;
			if (satz || (kurz && (w >= 2 || (imText && t.length >= MIN_WORT)))) aus.add(t);
		}
	}
	return [...aus];
}

/**
 * Die Fenster eines Textes: je vier Wörter mit mindestens 20 Zeichen; ein
 * kürzerer Text (höchstens vier Wörter) ganz.
 * @param {string} satz
 * @returns {string[]}
 */
export function fenster(satz) {
	const w = satz.split(' ');
	if (w.length <= FENSTER) return [satz];
	const aus = [];
	for (let i = 0; i + FENSTER <= w.length; i += 1) {
		const f = w.slice(i, i + FENSTER).join(' ');
		if (f.length >= MIN_ZEICHEN) aus.push(f);
	}
	return aus;
}

/** Wie oft `wort` als ganzes Wort (nicht Teil eines Slugs oder Namens) in `text` steht. */
export function wortZaehlen(/** @type {string} */ text, /** @type {string} */ wort) {
	let n = 0;
	let i = text.indexOf(wort);
	const teil = (/** @type {string | undefined} */ c) => c !== undefined && /[A-Za-z0-9_-]/.test(c);
	while (i >= 0) {
		if (!teil(text[i - 1]) && !teil(text[i + wort.length])) n += 1;
		i = text.indexOf(wort, i + 1);
	}
	return n;
}

function zaehlen(/** @type {string} */ text, /** @type {string} */ satz) {
	let n = 0;
	for (let i = text.indexOf(satz); i >= 0; i = text.indexOf(satz, i + 1)) n += 1;
	return n;
}

/** Alle Dateien unter `ordner`, relativ mit `/`. */
function alle(/** @type {string} */ ordner) {
	return readdirSync(ordner, { recursive: true, encoding: 'utf8' })
		.map((p) => p.replaceAll('\\', '/'))
		.filter((p) => statSync(join(ordner, p)).isFile());
}

/**
 * @param {{ bau: string, bauliste: string, huelle: string }} a
 *   `huelle`: der Text der Quellen der Hülle (web/src ohne Bauliste und Projekttypen)
 * @returns {{ stufe: 0 | 1 | 2, satz: string, oeffentlich: number, geschuetzt: number,
 *   slugs: number, fenster: number, frei: number, texte: number, gesehen: number,
 *   fehlend: number, freieSeiten: number, textlos: number, blindeSeiten: number,
 *   funde: { datei: string, anzahl: number }[] }}
 */
export function probe({ bau, bauliste, huelle }) {
	const leer = {
		oeffentlich: 0,
		geschuetzt: 0,
		slugs: 0,
		fenster: 0,
		frei: 0,
		texte: 0,
		gesehen: 0,
		fehlend: 0,
		freieSeiten: 0,
		textlos: 0,
		blindeSeiten: 0,
		funde: []
	};
	const listeDatei = join(bau, '_app', 'oeffentlich.json');
	if (!existsSync(listeDatei))
		return { ...leer, stufe: 2, satz: 'Der Bau hat keine _app/oeffentlich.json.' };
	/** @type {Set<string>} */
	let liste;
	try {
		liste = new Set(JSON.parse(readFileSync(listeDatei, 'utf8')));
	} catch {
		return { ...leer, stufe: 2, satz: 'Die _app/oeffentlich.json des Baus ist unlesbar.' };
	}
	const baulisteText = readFileSync(bauliste, 'utf8');
	const { slugs, dateien, seiten, unlesbar } = baulisteLesen(baulisteText);
	if (unlesbar !== 0) {
		return {
			...leer,
			stufe: 2,
			satz: `Die Bauliste hat ${Math.abs(unlesbar)} Einträge, die die Probe nicht lesen kann; sie wäre blind.`
		};
	}
	const huelleEben = eben(huelle);
	// Frei: steht auch in der Hülle (bei jedem Lauf abgeleitet, nie gespeichert).
	const freieSlugs = slugs.filter((s) => wortZaehlen(huelleEben, s) > 0);
	/** Seitendateien eines freien Slugs: dort zählen auch kurze Stücke. */
	const freiDateien = new Set(
		seiten.filter((s) => freieSlugs.includes(s.slug)).map((s) => s.datei)
	);
	const da = dateien.filter((d) => existsSync(d));
	/** Je Seitendatei ihre Texte, je Text die Fenster, die nicht in der Hülle stehen. */
	let texteJe;
	try {
		texteJe = new Map(
			da.map((d) => [
				d,
				satzfolgen(readFileSync(d, 'utf8'), freiDateien.has(d)).map((t) => ({
					t,
					f: fenster(t).filter((f) => !huelleEben.includes(f))
				}))
			])
		);
	} catch {
		return {
			...leer,
			stufe: 2,
			satz: 'Eine Seitendatei der Bauliste lässt sich nicht lesen; die Probe wäre blind.'
		};
	}
	const texte = [...new Map([...texteJe.values()].flat().map((x) => [x.t, x.f])).values()];
	const begriffe = [
		...slugs.filter((s) => !freieSlugs.includes(s)).map((wort) => ({ wort, slug: true })),
		...[...new Set(texte.flat())].map((wort) => ({ wort, slug: false }))
	];
	const fensterZahl = begriffe.length - (slugs.length - freieSlugs.length);
	const zaehle = (/** @type {string} */ text, /** @type {{ wort: string, slug: boolean }} */ b) =>
		b.slug ? wortZaehlen(text, b.wort) : zaehlen(text, b.wort);
	/** @type {{ datei: string, anzahl: number }[]} */
	const funde = [];
	let geschuetzt = 0;
	let oeffentlich = 0;
	const geschuetztText = [];
	for (const rel of alle(bau).filter((p) => TEXT.test(p))) {
		const text = eben(readFileSync(join(bau, rel), 'utf8'));
		if (rel.startsWith('_app/') && !liste.has(rel)) {
			geschuetzt += 1;
			geschuetztText.push(text);
			continue;
		}
		oeffentlich += 1;
		const anzahl = begriffe.reduce((s, b) => s + zaehle(text, b), 0);
		if (anzahl > 0) funde.push({ datei: rel, anzahl });
	}
	funde.sort((a, b) => a.datei.localeCompare(b.datei));
	// Ein Seitentext gilt als gesehen, wenn eins seiner Fenster in einer geschützten Datei steht.
	const g = geschuetztText.join('\n');
	/** Prüfbar (mit Fenstern außerhalb der Hülle) und davon gesehen. */
	const zaehlenGesehen = (/** @type {string[][]} */ t) => {
		const p = t.filter((f) => f.length > 0);
		return { p: p.length, s: p.filter((f) => f.some((x) => g.includes(x))).length };
	};
	const blind = (/** @type {{ p: number, s: number }} */ z) => z.p === 0 || z.s < MIN_GESEHEN * z.p;
	const gesamt = zaehlenGesehen(texte);
	// Seiten eines Slugs, der ein Wort der Hülle ist: jede für sich nicht blind.
	// Eine Seite ganz ohne Text, auch ohne kurze Stücke (etwa nur Bausteine mit
	// Daten), hat nichts, was die Probe suchen könnte; sie zählt als textlos.
	const freieSeiten = [...freiDateien].filter((d) => texteJe.has(d));
	const textlos = freieSeiten.filter((d) => (texteJe.get(d) ?? []).length === 0).length;
	const blindeSeiten = freieSeiten.filter(
		(d) =>
			(texteJe.get(d) ?? []).length > 0 &&
			blind(zaehlenGesehen((texteJe.get(d) ?? []).map((x) => x.f)))
	).length;
	const zahlen = {
		oeffentlich,
		geschuetzt,
		slugs: slugs.length,
		fenster: fensterZahl,
		frei: freieSlugs.length,
		texte: gesamt.p,
		gesehen: gesamt.s,
		fehlend: dateien.length - da.length,
		freieSeiten: freieSeiten.length,
		textlos,
		blindeSeiten,
		funde
	};
	if (slugs.length === 0) {
		// Die leere Tabelle ist höchstens eine geschützte Datei; mehr heißt: Der Bau
		// kam aus einer anderen Bauliste, oder die Liste hat eine fremde Form.
		const leerUndPasst =
			(baulisteText.match(LEERE_TABELLE) ?? []).length === 2 &&
			!/import\.meta|import\((?!'svelte'\))/.test(baulisteText) &&
			geschuetzt <= 1;
		return leerUndPasst
			? {
					...zahlen,
					stufe: 0,
					satz: 'Die Bauliste nennt keine Projektseiten; es gibt nichts zu schützen.'
				}
			: {
					...zahlen,
					stufe: 2,
					satz: `Die Bauliste nennt keine Projektseiten, aber der Bau hat ${geschuetzt} geschützte Dateien oder die Liste eine fremde Form; Bau und Bauliste passen nicht zusammen.`
				};
	}
	if (funde.length > 0)
		return {
			...zahlen,
			stufe: 1,
			satz: 'Öffentliche Dateien nennen Slugs oder Texte eigener Projektseiten.'
		};
	if (zahlen.fehlend > 0)
		return {
			...zahlen,
			stufe: 2,
			satz: `${zahlen.fehlend} Seitendateien der Bauliste fehlen; ihre Texte kann die Probe nicht suchen.`
		};
	if (blind(gesamt))
		return {
			...zahlen,
			stufe: 2,
			satz: 'Die Probe findet zu wenige Seitentexte in den geschützten Dateien; sie wäre blind.'
		};
	if (blindeSeiten > 0)
		return {
			...zahlen,
			stufe: 2,
			satz: `${blindeSeiten} Seiten eines Slugs, der zugleich ein Wort der Hülle ist, sieht die Probe nicht in den geschützten Dateien; für sie wäre sie blind.`
		};
	return {
		...zahlen,
		stufe: 0,
		satz: 'Keine öffentliche Datei nennt einen Slug oder Text einer Projektseite, außer Wörtern der Hülle.'
	};
}

/**
 * Der Text der Quellen der Hülle: alle Dateien unter `src` außer der Bauliste
 * und den Typen einzelner Projekte (segmentweise wie scripts/grundbau.mjs).
 * @param {string} src
 */
export function huellenText(src) {
	return alle(src)
		.filter(
			(p) =>
				p !== 'lib/projekte.generated.ts' &&
				p.split('/').slice(0, 3).join('|') !== 'lib|vertrag|projekte'
		)
		.filter((p) => /\.(ts|js|svelte|css|html)$/.test(p))
		.map((p) => readFileSync(join(src, p), 'utf8'))
		.join('\n');
}

/**
 * Der Aufruf von der Kommandozeile (im Ordner `web`); gibt die Rückgabe
 * zurück. Bei einem Fund heißt die Liste danach `oeffentlich.json.fund`:
 * Ohne Liste gibt der Dienst unter /_app/ ohne Anmeldung nichts heraus.
 * @param {string[]} args
 * @param {{ web?: string, env?: Record<string, string | undefined>, aus?: (z: string) => void }} [o]
 * @returns {0 | 1 | 2}
 */
export function hauptprogramm(args, { web = WEB, env = process.env, aus = console.log } = {}) {
	const start = performance.now();
	const ziel = args.find((a) => !a.startsWith('--')) ?? env.DASHBOARD_BUILD_ZIEL ?? 'build';
	const bau = resolve(web, ziel);
	const marke = join(bau, '_app', 'oeffentlich.blind');
	rmSync(marke, { force: true });
	const e = probe({
		bau,
		bauliste: join(web, 'src', 'lib', 'projekte.generated.ts'),
		huelle: huellenText(join(web, 'src'))
	});
	aus(`Probe der öffentlichen Dateien: ${e.satz}`);
	aus(
		`Öffentliche Dateien: ${e.oeffentlich}, geschützte: ${e.geschuetzt}; Slugs: ${e.slugs} (davon ${e.frei} Wörter der Hülle), ` +
			`Textfenster: ${e.fenster}; Seitentexte in geschützten Dateien gefunden: ${e.gesehen} von ${e.texte}; ` +
			`Seiten mit Slug als Wort der Hülle: ${e.freieSeiten}, davon ohne eigenen Text: ${e.textlos}, ungesehen: ${e.blindeSeiten}; fehlende Seitendateien: ${e.fehlend}.`
	);
	for (const f of e.funde) aus(`  ${f.datei}: ${f.anzahl} Fundstellen`);
	if (e.stufe === 1) {
		/** Umbenennen; geht das nicht, löschen (Bau-Ausgabe); geht auch das nicht, ein klarer Satz. */
		const weg = (/** @type {string} */ von, /** @type {string} */ nach) => {
			try {
				mkdirSync(dirname(nach), { recursive: true });
				renameSync(von, nach);
				return true;
			} catch (u) {
				try {
					rmSync(von, { force: true });
					aus(`  umbenennen ging nicht (${String(u)}); gelöscht: ${von}`);
					return true;
				} catch (l) {
					aus(`  WEDER UMBENANNT NOCH GELÖSCHT (${String(l)}): ${von}`);
					return false;
				}
			}
		};
		const liste = join(bau, '_app', 'oeffentlich.json');
		let zu = weg(liste, `${liste}.fund`);
		// Außerhalb von _app/ ist jede Datei öffentlich, auch unter anderem Namen; unter
		// _app/fund/ ist sie ohne Liste geschützt.
		for (const f of e.funde.filter((f) => !f.datei.startsWith('_app/')))
			zu = weg(join(bau, f.datei), join(bau, '_app', 'fund', f.datei)) && zu;
		// Die Skripte erkennen den Fund an oeffentlich.json.fund; wurde die Liste
		// gelöscht oder blieb sie liegen, steht dort eine Marke, sonst rieten sie zu -SkipWeb.
		if (!existsSync(`${liste}.fund`)) {
			try {
				writeFileSync(`${liste}.fund`, `${e.satz}\n`);
			} catch (u) {
				aus(`Die Marke ${liste}.fund ließ sich nicht schreiben (${String(u)}).`);
			}
		}
		aus(
			zu
				? 'Die Liste heißt jetzt _app/oeffentlich.json.fund (oder ist gelöscht), Fund-Dateien außerhalb von _app/ liegen unter _app/fund/ (oder sind gelöscht); ohne Anmeldung gibt dieser Bau sie nicht heraus.'
				: 'Der Bau ist NICHT verschlossen: Dateien oben ließen sich weder umbenennen noch löschen. Diesen Bau nicht ausliefern; den Bauordner von Hand löschen.'
		);
	}
	if (e.stufe === 2) {
		// Die Marke sagt den Skripten: Die Probe kann nicht prüfen; kein Fehler eines Projekts.
		try {
			mkdirSync(dirname(marke), { recursive: true });
			writeFileSync(marke, `${e.satz}\n`);
		} catch (u) {
			aus(`Die Marke ${marke} ließ sich nicht schreiben (${String(u)}).`);
		}
	}
	aus(`Laufzeit: ${((performance.now() - start) / 1000).toFixed(1)} s`);
	return e.stufe;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	process.exitCode = hauptprogramm(process.argv.slice(2));
}
