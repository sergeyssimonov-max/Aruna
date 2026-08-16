(function () {
  var input = document.getElementById("q");
  var table = document.getElementById("inv");
  var hint = document.getElementById("hint");
  var foldAll = document.getElementById("fold-all");
  if (!input || !table) return;

  var tbody = table.tBodies[0];
  var rows = Array.prototype.slice.call(tbody.rows);

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
    var text = (tr.textContent || "").toLowerCase();
    if (tr.classList.contains("group")) {
      current = { tr: tr, text: text, items: [], texts: [], folded: false };
      groups.push(current);
    } else if (current) {
      current.items.push(tr);
      current.texts.push(text);
    }
  }

  var COLLAPSE = "Collapse fragments";
  var EXPAND = "Expand fragments";

  /** Apply the current query and fold state to every row. */
  function render() {
    var q = (input.value || "").trim().toLowerCase();
    var matches = 0;

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
        if (show) matches++;
      }

      setFolded(group, group.folded);
    }

    if (q === "") {
      hint.textContent = "";
    } else {
      hint.textContent = matches ? "Match: " + matches.toLocaleString() : "No matches";
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
