/**
 * Randschatten für seitlich scrollende Rahmen (.scroll-x, .table-scroll).
 *
 * Setzt `schatten-links` bzw. `schatten-rechts`, solange es in diese
 * Richtung noch etwas zu sehen gibt. Ohne Überlauf bleibt der Rahmen leer;
 * es wird nichts in einer Deckfarbe gemalt, darum stimmt es auf jedem
 * Untergrund (--card wie --bg). Als Svelte-Aktion (`use:scrollShadow`)
 * oder direkt aufrufbar.
 */
export function scrollShadow(node: HTMLElement): { destroy: () => void } {
	const update = (): void => {
		const max = node.scrollWidth - node.clientWidth;
		const at = Math.abs(node.scrollLeft);
		const over = max > 1;
		node.classList.toggle('schatten-links', over && at > 1);
		node.classList.toggle('schatten-rechts', over && at < max - 1);
	};
	node.addEventListener('scroll', update, { passive: true });

	// Breite des Rahmens oder des Inhalts (neue Zeilen, andere Daten) ändert
	// den Überlauf; beides beobachten, Kinder auch nach Austausch.
	const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
	const watchChildren = (): void => {
		if (!ro) return;
		ro.disconnect();
		ro.observe(node);
		for (const c of node.children) ro.observe(c);
	};
	watchChildren();
	const mo =
		typeof MutationObserver === 'undefined'
			? null
			: new MutationObserver(() => {
					watchChildren();
					update();
				});
	mo?.observe(node, { childList: true });
	update();

	return {
		destroy(): void {
			node.removeEventListener('scroll', update);
			ro?.disconnect();
			mo?.disconnect();
			node.classList.remove('schatten-links', 'schatten-rechts');
		}
	};
}
