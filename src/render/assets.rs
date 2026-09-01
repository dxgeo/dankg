//! The HTML page's stylesheet and script, as string constants.
//!
//! Inlined verbatim by `html.rs`, so the rendered page is one file with no
//! network requests, no build step and no server. That is the whole reason
//! they live here as `const &str` rather than as files loaded at runtime: a
//! page that needs a second file beside it is not a single self-contained
//! artefact, and decision 1 leaves no bundler to solve that with.
//!
//! Neither string is generated. Both are hand-written, and both are held to
//! the same rule as the Rust: no dependencies, and nothing that phones home.

pub const CSS: &str = r##"
:root {
  color-scheme: light dark;
  --bg: #fbfbfa;
  --panel: #f2f2ef;
  --fg: #1c1c1b;
  --muted: #8c8c86;
  --rule: #dcdcd6;
  --node-bg: #ffffff;
  --node-line: #3c3c3c;
  --edge: #8f8f8f;
  --contains: #c4c4be;
  --accent: #2f6f4f;
  --accent-soft: #d9e8df;
  /* A named code block's own node -- distinct from an ordinary heading's
     --node-bg, the same distinction dot.rs draws with fillcolor and
     tui/draw.rs draws with a different border glyph. */
  --block-bg: #eef2ff;
  --block-line: #3c3c5c;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #17181a;
    --panel: #1f2124;
    --fg: #e6e6e2;
    --muted: #7e8188;
    --rule: #303338;
    --node-bg: #24262a;
    --node-line: #9aa0a8;
    --edge: #6d727a;
    --contains: #3c4046;
    --accent: #74c39a;
    --accent-soft: #24382e;
    --block-bg: #23263a;
    --block-line: #9a9ac8;
  }
}

* { box-sizing: border-box; }

html, body {
  margin: 0;
  height: 100%;
}

body {
  display: flex;
  flex-direction: column;
  background: var(--bg);
  color: var(--fg);
  font: 13px/1.5 ui-sans-serif, system-ui, -apple-system, "Helvetica Neue", Arial, sans-serif;
}

header {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px 18px;
  padding: 9px 14px;
  border-bottom: 1px solid var(--rule);
  background: var(--panel);
}

header .root {
  font-weight: 600;
}

header .counts,
header .legend {
  color: var(--muted);
}

header .legend {
  display: flex;
  gap: 14px;
  align-items: center;
}

header .legend span {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

header .legend i {
  width: 22px;
  height: 0;
  border-top: 2px solid var(--edge);
}

header .legend i.contains { border-top-color: var(--contains); border-top-width: 3px; }
header .legend i.dangling { border-top-style: dashed; border-top-color: var(--muted); }

header .controls {
  margin-left: auto;
  display: flex;
  gap: 6px;
}

button {
  font: inherit;
  color: inherit;
  background: var(--bg);
  border: 1px solid var(--rule);
  border-radius: 5px;
  padding: 2px 9px;
  cursor: pointer;
}

button:hover { border-color: var(--accent); }
button:active { background: var(--accent-soft); }

main {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

svg.canvas {
  display: block;
  width: 100%;
  height: 100%;
  cursor: grab;
  touch-action: none;
}

svg.canvas.panning { cursor: grabbing; }

.edge {
  fill: none;
  stroke: var(--edge);
  stroke-width: 1.4;
}

.edge.contains {
  stroke: var(--contains);
  stroke-width: 2.2;
}

.edge.dangling { stroke-dasharray: 4 3; }

/* Arrowheads are filled, not stroked, and a presentation attribute cannot
   resolve a custom property -- so the colour has to be matched here. */
#tip path { fill: var(--edge); }
#tip-contains path { fill: var(--contains); }

.node .box {
  fill: var(--node-bg);
  stroke: var(--node-line);
  stroke-width: 1.2;
  rx: 6;
}

.node .label {
  fill: var(--fg);
  font-size: 12px;
  text-anchor: middle;
  dominant-baseline: central;
  pointer-events: none;
}

.node.unresolved .box {
  stroke: var(--muted);
  stroke-dasharray: 5 3;
}

.node.unresolved .label { fill: var(--muted); }

.node.block .box {
  fill: var(--block-bg);
  stroke: var(--block-line);
}

.node.block .label { font-family: ui-monospace, "SF Mono", Menlo, monospace; }

.node.entry .box {
  stroke: var(--accent);
  stroke-width: 2.2;
}

.node.expandable .box { cursor: pointer; }
.node.expandable:hover .box { stroke: var(--accent); }

.node[data-expanded="true"] .box { fill: var(--accent-soft); }

/* Grown by the script rather than served by Rust, so it reads as provisional:
   this is a local placement, not a re-run of the layout. Nothing here touches
   the dashes -- an expanded placeholder is still a placeholder. */
.node.grown .label { font-style: italic; }

.src circle {
  fill: var(--panel);
  stroke: var(--rule);
}

.src text {
  fill: var(--muted);
  font-size: 10px;
  text-anchor: middle;
  dominant-baseline: central;
}

.src {
  opacity: 0;
  transition: opacity 90ms linear;
}

.node:hover .src,
.src:focus-visible {
  opacity: 1;
}

.src:hover circle { stroke: var(--accent); }
.src:hover text { fill: var(--accent); }

footer {
  padding: 6px 14px;
  border-top: 1px solid var(--rule);
  background: var(--panel);
  color: var(--muted);
}

footer b { font-weight: 600; color: var(--fg); }
"##;

pub const JS: &str = r##"
(function () {
  "use strict";

  var svg = document.getElementById("graph");
  var scene = document.getElementById("scene");
  var edgeLayer = document.getElementById("edges");
  var nodeLayer = document.getElementById("nodes");
  var status = document.getElementById("status");

  var meta = JSON.parse(document.getElementById("dankg-meta").textContent);
  var index = JSON.parse(document.getElementById("dankg-index").textContent);
  var G = meta.geometry;
  var SVGNS = "http://www.w3.org/2000/svg";

  // ---- the index, in the shapes this file actually asks questions in -------

  var byId = new Map();
  index.nodes.forEach(function (n) { byId.set(n.id, n); });

  // id -> neighbours, each carrying which way the author wrote the edge.
  var around = new Map();
  function adjoin(id, entry) {
    var list = around.get(id);
    if (!list) { around.set(id, [entry]); } else { list.push(entry); }
  }
  index.edges.forEach(function (e) {
    adjoin(e.from, { other: e.to, out: true, edge: e });
    if (e.to !== e.from) { adjoin(e.to, { other: e.from, out: false, edge: e }); }
  });

  // A reciprocated pair is one line, so both halves have to agree on a key or
  // the second would be drawn over the first -- and so must this file and the
  // renderer, which stamps `data-key` on every line it draws. The separator
  // comes from the page rather than being written down again here: the two
  // halves are in different languages and nothing else would catch the drift.
  var SEP = meta.edgeKeySep;
  function edgeKey(e) {
    if (e.kind === "link" && e.reciprocated && e.from !== e.to) {
      return (e.from < e.to ? e.from + SEP + e.to : e.to + SEP + e.from) + SEP + "link";
    }
    return e.from + SEP + e.to + SEP + e.kind;
  }

  // ---- what is on screen ---------------------------------------------------

  var visible = new Map();   // id -> {el, rank, x, y, w, owner}
  var expanded = new Set();
  var drawnEdges = new Map();  // key -> element

  Array.prototype.forEach.call(nodeLayer.querySelectorAll(".node"), function (el) {
    visible.set(el.dataset.id, {
      el: el,
      rank: +el.dataset.rank,
      x: +el.dataset.x,
      y: +el.dataset.y,
      w: +el.dataset.w,
      owner: null
    });
  });
  Array.prototype.forEach.call(edgeLayer.querySelectorAll(".edge"), function (el) {
    drawnEdges.set(el.dataset.key, el);
  });

  function hiddenNeighbours(id) {
    return (around.get(id) || []).filter(function (n) {
      return !visible.has(n.other) && byId.has(n.other);
    });
  }

  function markExpandable() {
    visible.forEach(function (v, id) {
      v.el.classList.toggle("expandable", hiddenNeighbours(id).length > 0 || expanded.has(id));
    });
  }

  function report() {
    status.innerHTML = "<b>" + visible.size + "</b> of " + index.nodes.length +
      " node(s) shown, <b>" + drawnEdges.size + "</b> of " + index.edges.length + " edge(s)";
  }

  // ---- placing a node the reader asked for --------------------------------
  //
  // Not a re-run of the layout: the ranks Rust computed are kept as a grid, and
  // a revealed node is dropped into the nearest free slot on the rank its edge
  // puts it. Sugiyama over the expanded set would move every box on screen,
  // which is exactly what a reader tracing a link does not want. `--depth N+1`
  // is how you get the real layout of the larger graph.

  function widthOf(title) {
    var estimate = Array.from(title).length * G.charWidth + G.labelPad;
    return Math.min(Math.max(estimate, G.minWidth), G.maxWidth);
  }

  function fitLabel(title, width) {
    var chars = Array.from(title);
    var capacity = Math.max(1, Math.floor((width - G.labelPad) / G.charWidth));
    if (chars.length <= capacity) { return title; }
    if (capacity <= 1) { return "…"; }
    return chars.slice(0, capacity - 1).join("") + "…";
  }

  function yOfRank(rank) {
    return G.margin + rank * (G.nodeHeight + G.rankSep) + G.nodeHeight / 2;
  }

  function freeSlot(rank, width, prefer) {
    var boxes = [];
    visible.forEach(function (v) {
      if (v.rank === rank) { boxes.push([v.x - v.w / 2, v.x + v.w / 2]); }
    });

    var fits = function (x) {
      return boxes.every(function (b) {
        return x + width / 2 + G.nodeSep <= b[0] || x - width / 2 - G.nodeSep >= b[1];
      });
    };

    var tries = [prefer];
    boxes.forEach(function (b) {
      tries.push(b[1] + G.nodeSep + width / 2);
      tries.push(b[0] - G.nodeSep - width / 2);
    });
    tries.sort(function (a, b) { return Math.abs(a - prefer) - Math.abs(b - prefer); });

    for (var i = 0; i < tries.length; i++) {
      var x = Math.round(tries[i]);
      if (fits(x)) { return x; }
    }
    // Nothing between the existing boxes was wide enough; go past all of them.
    var right = boxes.reduce(function (acc, b) { return Math.max(acc, b[1]); }, G.margin);
    return Math.round(right + G.nodeSep + width / 2);
  }

  function el(name, attrs) {
    var node = document.createElementNS(SVGNS, name);
    Object.keys(attrs).forEach(function (k) { node.setAttribute(k, attrs[k]); });
    return node;
  }

  function makeNode(data, rank, x, y, width) {
    var extra = !data.resolved ? " unresolved" : (data.kind === "block" ? " block" : "");
    var g = el("g", { class: "node grown" + extra });
    g.dataset.id = data.id;
    g.dataset.rank = rank;
    g.dataset.x = x;
    g.dataset.y = y;
    g.dataset.w = width;

    var tip = el("title", {});
    tip.textContent = data.resolved ? data.file + ":" + data.line : data.id + " (unresolved)";
    g.appendChild(tip);
    g.appendChild(el("rect", {
      class: "box",
      x: x - width / 2,
      y: y - G.nodeHeight / 2,
      width: width,
      height: G.nodeHeight
    }));

    var label = el("text", { class: "label", x: x, y: y });
    label.textContent = fitLabel(data.title, width);
    g.appendChild(label);

    if (data.resolved) {
      var link = el("a", { class: "src", href: data.file, target: "_blank", rel: "noopener" });
      var linkTip = el("title", {});
      linkTip.textContent = "open " + data.file + ":" + data.line;
      link.appendChild(linkTip);
      link.appendChild(el("circle", { cx: x + width / 2, cy: y - G.nodeHeight / 2, r: 7 }));
      var glyph = el("text", { x: x + width / 2, y: y - G.nodeHeight / 2 });
      glyph.textContent = "↗";
      link.appendChild(glyph);
      g.appendChild(link);
    }
    return g;
  }

  function edgePath(from, to) {
    var half = G.nodeHeight / 2;
    if (from === to) {
      var out = from.x + from.w / 2 + G.nodeSep;
      return "M" + from.x + "," + (from.y + half) + " L" + out + "," + (from.y + half) +
        " L" + out + "," + (from.y - half) + " L" + from.x + "," + (from.y - half);
    }
    if (to.y === from.y) {
      // Same rank: leave and enter the sides, so the line is not swallowed.
      var dir = to.x > from.x ? 1 : -1;
      return "M" + (from.x + dir * from.w / 2) + "," + from.y +
        " L" + (to.x - dir * to.w / 2) + "," + to.y;
    }
    var down = to.y > from.y;
    return "M" + from.x + "," + (from.y + (down ? half : -half)) +
      " L" + to.x + "," + (to.y + (down ? -half : half));
  }

  function syncEdges() {
    index.edges.forEach(function (e) {
      var key = edgeKey(e);
      if (drawnEdges.has(key)) { return; }
      var from = visible.get(e.from);
      var to = visible.get(e.to);
      if (!from || !to) { return; }

      var classes = ["edge", e.kind];
      if (!byId.get(e.from).resolved || !byId.get(e.to).resolved) { classes.push("dangling"); }
      var path = el("path", { class: classes.join(" "), d: edgePath(from, to) });
      path.dataset.key = key;
      path.dataset.from = e.from;
      path.dataset.to = e.to;
      // Reciprocated means mutual, and a mutual link has no one direction to
      // point in -- the same rule the static half of the drawing follows.
      if (!(e.kind === "link" && e.reciprocated && e.from !== e.to)) {
        path.setAttribute("marker-end", e.kind === "contains" ? "url(#tip-contains)" : "url(#tip)");
      }
      edgeLayer.appendChild(path);
      drawnEdges.set(key, path);
    });
  }

  function expand(id) {
    var anchor = visible.get(id);
    if (!anchor) { return; }
    expanded.add(id);
    anchor.el.dataset.expanded = "true";

    hiddenNeighbours(id).forEach(function (n) {
      if (visible.has(n.other)) { return; }   // an earlier neighbour brought it
      var data = byId.get(n.other);
      var rank = anchor.rank + (n.out ? 1 : -1);
      var width = widthOf(data.title);
      var x = freeSlot(rank, width, anchor.x);
      var y = yOfRank(rank);
      var node = makeNode(data, rank, x, y, width);
      nodeLayer.appendChild(node);
      visible.set(data.id, { el: node, rank: rank, x: x, y: y, w: width, owner: id });
    });
    syncEdges();
  }

  function removeNode(id) {
    var v = visible.get(id);
    if (!v) { return; }
    v.el.remove();
    visible.delete(id);
    expanded.delete(id);
    drawnEdges.forEach(function (path, key) {
      if (path.dataset.from === id || path.dataset.to === id) {
        path.remove();
        drawnEdges.delete(key);
      }
    });
  }

  function collapse(id) {
    var anchor = visible.get(id);
    expanded.delete(id);
    if (anchor) { delete anchor.el.dataset.expanded; }

    Array.from(visible.keys()).forEach(function (other) {
      var v = visible.get(other);
      if (!v || v.owner !== id) { return; }

      // Somebody else the reader expanded also reaches it, so it is not this
      // node's to take away. Hand it over rather than making it vanish.
      var keeper = null;
      expanded.forEach(function (e) {
        if (keeper || e === id) { return; }
        if ((around.get(e) || []).some(function (n) { return n.other === other; })) { keeper = e; }
      });
      if (keeper) { v.owner = keeper; return; }

      if (expanded.has(other)) { collapse(other); }
      removeNode(other);
    });
  }

  function toggle(id) {
    if (expanded.has(id)) { collapse(id); } else { expand(id); }
    markExpandable();
    report();
  }

  // ---- pan and zoom --------------------------------------------------------

  var view = { k: 1, x: 0, y: 0 };

  function apply() {
    scene.setAttribute("transform", "translate(" + view.x + " " + view.y + ") scale(" + view.k + ")");
  }

  /// Client coordinates in the viewBox's units, which is what `view` is in.
  function inCanvas(event) {
    var box = svg.getBoundingClientRect();
    var scale = Math.min(box.width / meta.width, box.height / meta.height);
    return {
      x: (event.clientX - box.left - (box.width - meta.width * scale) / 2) / scale,
      y: (event.clientY - box.top - (box.height - meta.height * scale) / 2) / scale
    };
  }

  function zoomTo(k, at) {
    k = Math.min(Math.max(k, 0.1), 6);
    view.x = at.x - (at.x - view.x) * (k / view.k);
    view.y = at.y - (at.y - view.y) * (k / view.k);
    view.k = k;
    apply();
  }

  svg.addEventListener("wheel", function (e) {
    e.preventDefault();
    zoomTo(view.k * Math.exp(-e.deltaY * 0.0015), inCanvas(e));
  }, { passive: false });

  var drag = null;

  svg.addEventListener("pointerdown", function (e) {
    if (e.button !== 0 || e.target.closest("a")) { return; }
    drag = { at: inCanvas(e), from: { x: view.x, y: view.y }, moved: false };
    svg.setPointerCapture(e.pointerId);
    svg.classList.add("panning");
  });

  svg.addEventListener("pointermove", function (e) {
    if (!drag) { return; }
    var at = inCanvas(e);
    view.x = drag.from.x + (at.x - drag.at.x) * view.k;
    view.y = drag.from.y + (at.y - drag.at.y) * view.k;
    if (Math.abs(at.x - drag.at.x) + Math.abs(at.y - drag.at.y) > 3) { drag.moved = true; }
    apply();
  });

  svg.addEventListener("pointerup", function (e) {
    svg.classList.remove("panning");
    if (!drag) { return; }
    var wasDrag = drag.moved;
    drag = null;
    if (wasDrag || e.target.closest("a")) { return; }
    var node = e.target.closest(".node");
    if (node) { toggle(node.dataset.id); }
  });

  svg.addEventListener("pointercancel", function () {
    drag = null;
    svg.classList.remove("panning");
  });

  function fit() {
    var box = scene.getBBox();
    if (!box.width || !box.height) { return; }
    var k = Math.min(meta.width / box.width, meta.height / box.height) * 0.94;
    view.k = Math.min(Math.max(k, 0.1), 6);
    view.x = meta.width / 2 - view.k * (box.x + box.width / 2);
    view.y = meta.height / 2 - view.k * (box.y + box.height / 2);
    apply();
  }

  function reset() {
    Array.from(expanded).forEach(collapse);
    view = { k: 1, x: 0, y: 0 };
    apply();
    markExpandable();
    report();
  }

  document.getElementById("fit").addEventListener("click", fit);
  document.getElementById("reset").addEventListener("click", reset);

  document.addEventListener("keydown", function (e) {
    if (e.target !== document.body && e.target !== svg) { return; }
    var centre = { x: meta.width / 2, y: meta.height / 2 };
    if (e.key === "+" || e.key === "=") { zoomTo(view.k * 1.2, centre); }
    else if (e.key === "-") { zoomTo(view.k / 1.2, centre); }
    else if (e.key === "0") { fit(); }
    else if (e.key === "Escape") { reset(); }
  });

  markExpandable();
  report();
})();
"##;
