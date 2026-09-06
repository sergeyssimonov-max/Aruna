(function() {
	//#region src/inventory/filter.ts
	/**
	* The inventory's client script: the search box and the fold controls.
	*
	* This is the first part of the exported document to be authored in the
	* frontend stack rather than written by hand — see `docs/FRONTEND-CONTRACT.md`,
	* *The target state*. Vite bundles it at build time into
	* `cli/src/generated/inventory_filter.js`, which the Rust binary compiles in
	* with `include_str!` exactly as it compiled in the hand-written file before
	* it. Nothing here runs in the desktop window: it runs in whatever browser
	* opens `TLHdig_Beta_0.3.html`, which may be offline and is certainly not
	* running Node.
	*
	* The document works without it. Without this script the table is a plain,
	* fully expanded list, the search box swallows what is typed, and the fold
	* button folds nothing — which is why the controls are only admitted to the
	* page (`filter-on`) once this has run.
	*/
	/**
	* The cell that carries the row's position in the table rather than its data.
	*
	* Asked for by the class the crate puts on it, not by its index. The order of
	* the columns is declared once, in `cli/src/html.rs`, and an index written here
	* was a second copy of that order with nothing holding the two together: a
	* column inserted before this one would have sent the search quietly looking at
	* the wrong cell.
	*/
	var ROW_NUMBER_CELL = ".num";
	var COLLAPSE = "Collapse fragments";
	var EXPAND = "Expand fragments";
	/**
	* The text a manuscript row is searched by: what it says, not where it sits.
	*
	* The row's own `textContent` was used here, and it begins with the ordinal in
	* the first cell — so `12345` matched row 12 345, whose fields contain no such
	* number, and any four-digit year also reached the rows numbered after it. The
	* ordinal is typography: it renumbers itself whenever the corpus grows, and
	* nothing in the archive answers to it.
	*
	* Cells are joined with a newline rather than a space, so a query cannot run
	* from the end of one column into the start of the next.
	*
	* The other spellings of an editor are in the text already: the crate writes
	* them into the row after the visible name, out of sight, and `textContent`
	* reads a hidden element like any other. Which spellings name one and the same
	* person is a fact about the corpus, and it was stated here until 2026-09-06 —
	* see `presentation::EDITOR_ALIASES`, where the crate that renders the cell now
	* says it once.
	*/
	function rowText(tr) {
		const parts = [];
		for (const cell of tr.cells) {
			var _cell$textContent;
			if (cell.matches(ROW_NUMBER_CELL)) continue;
			parts.push((_cell$textContent = cell.textContent) !== null && _cell$textContent !== void 0 ? _cell$textContent : "");
		}
		return parts.join("\n").toLowerCase();
	}
	/**
	* The text a heading is searched by: the catalogue number it names.
	*
	* Its `textContent` also carries the tally beside the label, with nothing in
	* between — the heading of `CTH 1` with six manuscripts reads `CTH 16`. A
	* search for `CTH 16` therefore opened `CTH 1` and counted all six of its
	* manuscripts as matches, and `CTH 316` did the same to `CTH 3`.
	*/
	function groupText(tr) {
		var _ref;
		const label = tr.querySelector(".group-label");
		return ((_ref = label ? label.textContent : tr.textContent) !== null && _ref !== void 0 ? _ref : "").toLowerCase();
	}
	function setFolded(group, folded) {
		group.folded = folded;
		group.tr.classList.toggle("folded", folded);
		const button = group.tr.querySelector(".group-toggle");
		if (button) button.setAttribute("aria-expanded", folded ? "false" : "true");
	}
	/**
	* Wire the search box and the fold controls to the table.
	*
	* Does nothing at all if the document holds no search box or no table, which
	* is what the CLI writes when there is nothing to inventory.
	*/
	function attachInventoryFilter(doc) {
		const input = doc.getElementById("q");
		const table = doc.getElementById("inv");
		const hint = doc.getElementById("hint");
		const foldAll = doc.getElementById("fold-all");
		if (!input || !table) return;
		const tbody = table.tBodies[0];
		const rows = Array.prototype.slice.call(tbody.rows);
		doc.body.classList.add("filter-on");
		const groups = [];
		let current = null;
		for (const tr of rows) if (tr.classList.contains("group")) {
			current = {
				tr,
				text: groupText(tr),
				items: [],
				texts: [],
				folded: false
			};
			groups.push(current);
		} else if (current) {
			current.items.push(tr);
			current.texts.push(rowText(tr));
		}
		/** The toolbar button folds everything, or opens everything back up. */
		function syncFoldAll() {
			if (!foldAll) return;
			const anyOpen = groups.some((group) => !group.folded);
			foldAll.textContent = anyOpen ? COLLAPSE : EXPAND;
			foldAll.setAttribute("aria-expanded", anyOpen ? "true" : "false");
		}
		/** Apply the current query and fold state to every row. */
		function render() {
			const q = (input.value || "").trim().toLowerCase();
			let matches = 0;
			let onScreen = 0;
			for (const group of groups) {
				const labelHit = q !== "" && group.text.includes(q);
				let anyHit = labelHit;
				const hits = [];
				for (let i = 0; i < group.items.length; i++) {
					const hit = q === "" || labelHit || group.texts[i].includes(q);
					hits.push(hit);
					if (hit) anyHit = true;
				}
				const groupVisible = q === "" || anyHit;
				group.tr.hidden = !groupVisible;
				for (let j = 0; j < group.items.length; j++) {
					const show = groupVisible && hits[j];
					group.items[j].hidden = !show || group.folded;
					if (show) {
						matches++;
						if (!group.folded) onScreen++;
					}
				}
				setFolded(group, group.folded);
			}
			if (hint) {
				if (q === "") hint.textContent = "";
				else if (!matches) hint.textContent = "No matches";
				else if (onScreen === matches) hint.textContent = "Match: " + matches.toLocaleString();
				else hint.textContent = "Match: " + matches.toLocaleString() + " · " + onScreen.toLocaleString() + " shown";
			}
			syncFoldAll();
		}
		input.addEventListener("input", render);
		if (foldAll) foldAll.addEventListener("click", () => {
			const fold = foldAll.textContent === COLLAPSE;
			for (const group of groups) setFolded(group, fold);
			render();
		});
		tbody.addEventListener("click", (event) => {
			const target = event.target;
			const button = target && target.closest ? target.closest(".group-toggle") : null;
			if (!button) return;
			const row = button.closest("tr");
			for (const group of groups) if (group.tr === row) {
				setFolded(group, !group.folded);
				render();
				return;
			}
		});
		render();
	}
	//#endregion
	//#region src/inventory/main.ts
	/**
	* The entry Vite bundles into the exported inventory.
	*
	* One line, on purpose: everything the script does is in `filter.ts`, where a
	* test can drive it against a document it built itself. This file is the only
	* part that assumes it is running in the page.
	*/
	attachInventoryFilter(document);
	//#endregion
})();
