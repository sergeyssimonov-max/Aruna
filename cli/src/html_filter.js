(function () {
  var input = document.getElementById("q");
  var table = document.getElementById("inv");
  var hint = document.getElementById("hint");
  var foldAll = document.getElementById("fold-all");
  if (!input || !table) return;

  var tbody = table.tBodies[0];
  var rows = Array.prototype.slice.call(tbody.rows);

  // The controls work from here on, so this is where they earn their place on
  // the page. Without this script they did nothing at all: a search box that
  // swallowed what you typed and a button that never folded anything.
  document.body.classList.add("filter-on");

  /** Which cell of a manuscript row names the editor. */
  var EDITOR_CELL = 4;

  /** Which cell carries the row's position in the table rather than its data. */
  var ROW_NUMBER_CELL = 0;

  // Spellings the corpus uses for one and the same editor, lowercased.
  //
  // TLHdig records who made an edition in whatever form the file's author
  // typed: initials in most documents, a full name in the newer ones. A search
  // for a surname would otherwise miss every row that carries only initials —
  // `schwemer` found 84 manuscripts and not the 7 that say `DS`.
  //
  // The same table drives the site's search index, where the evidence for each
  // group is set out in full; `editor-aliases-agreement.test.ts` fails if the
  // two ever list different people. What an alias changes is which rows a query
  // reaches — never what a row displays, which stays as the document wrote it.
  var EDITOR_ALIASES = [["ds", "daniel schwemer"], ["ff", "francesco fuscagni"]];

  /**
   * The other spellings of whoever this row credits, ready to append to the
   * text it is searched by.
   *
   * Matched against the Editor cell alone, and in full: `ds` as a substring of
   * the whole row would catch every `CHDS` siglum in the corpus, which is 1 059
   * manuscripts having nothing to do with the person.
   */
  function aliasesOf(tr) {
    var cell = tr.cells[EDITOR_CELL];
    var editor = cell ? (cell.textContent || "").trim().toLowerCase() : "";
    if (!editor) return "";
    for (var a = 0; a < EDITOR_ALIASES.length; a++) {
      var group = EDITOR_ALIASES[a];
      if (group.indexOf(editor) === -1) continue;
      var others = [];
      for (var s = 0; s < group.length; s++) {
        if (group[s] !== editor) others.push(group[s]);
      }
      return "\n" + others.join("\n");
    }
    return "";
  }

  /**
   * The text a manuscript row is searched by: what it says, not where it sits.
   *
   * The row's own `textContent` was used here, and it begins with the ordinal
   * in the first cell — so `12345` matched row 12 345, whose fields contain no
   * such number, and any four-digit year also reached the rows numbered after
   * it. The ordinal is typography: it renumbers itself whenever the corpus
   * grows, and nothing in the archive answers to it.
   *
   * Cells are joined with a newline rather than a space, so a query cannot run
   * from the end of one column into the start of the next. That is how the
   * site's index is built too.
   */
  function rowText(tr) {
    var parts = [];
    for (var c = 0; c < tr.cells.length; c++) {
      if (c === ROW_NUMBER_CELL) continue;
      parts.push(tr.cells[c].textContent || "");
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
    var label = tr.querySelector(".group-label");
    return ((label ? label.textContent : tr.textContent) || "").toLowerCase();
  }

  // One entry per CTH group: its heading row, its manuscripts, the lowercased
  // text each of them is searched by, and whether the group is folded shut.
  //
  // Built once. Everything below decides visibility from this and the current
  // query, and never reads the DOM to find out what is on screen — the two
  // controls would otherwise disagree about rows they had each hidden.
  var groups = [];
  var current = null;
  for (var i = 0; i < rows.length; i++) {
    var tr = rows[i];
    if (tr.classList.contains("group")) {
      current = { tr: tr, text: groupText(tr), items: [], texts: [], folded: false };
      groups.push(current);
    } else if (current) {
      current.items.push(tr);
      current.texts.push(rowText(tr) + aliasesOf(tr));
    }
  }

  var COLLAPSE = "Collapse fragments";
  var EXPAND = "Expand fragments";

  /** Apply the current query and fold state to every row. */
  function render() {
    var q = (input.value || "").trim().toLowerCase();
    var matches = 0;
    var onScreen = 0;

    for (var g = 0; g < groups.length; g++) {
      var group = groups[g];
      var labelHit = q !== "" && group.text.indexOf(q) !== -1;
      var anyHit = labelHit;
      var hits = [];

      for (var i = 0; i < group.items.length; i++) {
        // A group whose own label matches stands for all of its manuscripts.
        var hit = q === "" || labelHit || group.texts[i].indexOf(q) !== -1;
        hits.push(hit);
        if (hit) anyHit = true;
      }

      var groupVisible = q === "" || anyHit;
      group.tr.hidden = !groupVisible;

      for (var j = 0; j < group.items.length; j++) {
        var show = groupVisible && hits[j];
        // Folding hides the manuscripts, not the heading: a folded group still
        // shows that it matched, and its count says how much is inside.
        group.items[j].hidden = !show || group.folded;
        if (show) {
          matches++;
          if (!group.folded) onScreen++;
        }
      }

      setFolded(group, group.folded);
    }

    if (q === "") {
      hint.textContent = "";
    } else if (!matches) {
      hint.textContent = "No matches";
    } else if (onScreen === matches) {
      hint.textContent = "Match: " + matches.toLocaleString();
    } else {
      // Folding can hide rows this query found, and the count alone then
      // described a table the reader was not looking at: collapse everything,
      // search, and it said "Match: 84" over an empty list.
      hint.textContent =
        "Match: " + matches.toLocaleString() + " · " + onScreen.toLocaleString() + " shown";
    }
    syncFoldAll();
  }

  function setFolded(group, folded) {
    group.folded = folded;
    group.tr.classList.toggle("folded", folded);
    var button = group.tr.querySelector(".group-toggle");
    if (button) button.setAttribute("aria-expanded", folded ? "false" : "true");
  }

  /** The toolbar button folds everything, or opens everything back up. */
  function syncFoldAll() {
    if (!foldAll) return;
    var anyOpen = false;
    for (var g = 0; g < groups.length; g++) {
      if (!groups[g].folded) {
        anyOpen = true;
        break;
      }
    }
    foldAll.textContent = anyOpen ? COLLAPSE : EXPAND;
    foldAll.setAttribute("aria-expanded", anyOpen ? "true" : "false");
  }

  input.addEventListener("input", render);

  if (foldAll) {
    foldAll.addEventListener("click", function () {
      // Whatever the mixture, one press makes it uniform: fold all if anything
      // is open, otherwise open all.
      var fold = foldAll.textContent === COLLAPSE;
      for (var g = 0; g < groups.length; g++) setFolded(groups[g], fold);
      render();
    });
  }

  // One listener on the table rather than 663 on the headings.
  tbody.addEventListener("click", function (event) {
    var button = event.target.closest ? event.target.closest(".group-toggle") : null;
    if (!button) return;
    var row = button.closest("tr");
    for (var g = 0; g < groups.length; g++) {
      if (groups[g].tr === row) {
        setFolded(groups[g], !groups[g].folded);
        render();
        return;
      }
    }
  });

  render();
})();
