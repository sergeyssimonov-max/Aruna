(function () {
  var input = document.getElementById("q");
  var table = document.getElementById("inv");
  var hint = document.getElementById("hint");
  if (!input || !table) return;
  var tbody = table.tBodies[0];
  var rows = Array.prototype.slice.call(tbody.rows);
  // Precompute searchable text + group membership
  var meta = rows.map(function (tr) {
    var isGroup = tr.classList.contains("group");
    return {
      tr: tr,
      isGroup: isGroup,
      text: (tr.textContent || "").toLowerCase(),
    };
  });
  function run() {
    var q = (input.value || "").trim().toLowerCase();
    if (!q) {
      // `var` is function-scoped, so this counter must not reuse the name of
      // the cursor declared below — it is the same binding, not a fresh one.
      for (var r = 0; r < meta.length; r++) meta[r].tr.hidden = false;
      hint.textContent = "";
      return;
    }
    var visible = 0;
    var i = 0;
    while (i < meta.length) {
      var m = meta[i];
      if (m.isGroup) {
        // group + following item rows until next group
        var j = i + 1;
        var any = m.text.indexOf(q) !== -1;
        var itemHits = [];
        while (j < meta.length && !meta[j].isGroup) {
          var hit = meta[j].text.indexOf(q) !== -1;
          itemHits.push(hit);
          if (hit) any = true;
          j++;
        }
        m.tr.hidden = !any;
        for (var k = 0; k < itemHits.length; k++) {
          var show = any && (m.text.indexOf(q) !== -1 || itemHits[k]);
          meta[i + 1 + k].tr.hidden = !show;
          if (show) visible++;
        }
        i = j;
      } else {
        i++;
      }
    }
    hint.textContent = visible ? "Match: " + visible.toLocaleString() : "No matches";
  }
  input.addEventListener("input", run);
})();
