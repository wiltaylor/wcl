// wdoc dopesheet player.
//
// Every `<g class="wdoc-dopesheet">` carries the sprite-sheet frame
// geometry as `data-dope-*` attributes and contains a nested
// `<svg class="dope-frame">` whose `viewBox` windows into the sheet. We
// advance that window one frame at a time at the authored fps: frame `k`
// maps to flat index `from + k`, then to a (col, row) in the sheet grid,
// then to the source rect `(ox + col*sx, oy + row*sy, fw, fh)`. No
// framework, no dependencies. Loaded with `defer`, so the DOM is ready.
(function () {
  "use strict";

  function num(el, name, fallback) {
    var v = parseFloat(el.getAttribute(name));
    return isFinite(v) ? v : fallback;
  }

  function setup(g) {
    var frame = g.querySelector("svg.dope-frame");
    if (!frame) return;

    var cols = num(g, "data-dope-cols", 1);
    var fw = num(g, "data-dope-fw", 0);
    var fh = num(g, "data-dope-fh", 0);
    if (cols < 1 || fw <= 0 || fh <= 0) return;
    var ox = num(g, "data-dope-ox", 0);
    var oy = num(g, "data-dope-oy", 0);
    var sx = num(g, "data-dope-sx", fw);
    var sy = num(g, "data-dope-sy", fh);
    var from = num(g, "data-dope-from", 0);
    var to = num(g, "data-dope-to", from);
    var fps = num(g, "data-dope-fps", 12);
    if (fps <= 0) fps = 12;
    var loop = g.getAttribute("data-dope-loop") === "1";
    var autoplay = g.getAttribute("data-dope-autoplay") === "1";

    var count = to - from + 1;
    if (count < 1) count = 1;
    var dt = 1000 / fps; // ms per frame

    var btn = g.querySelector(".dope-btn");
    var i = 0;
    var playing = false;
    var ended = false;
    var raf = null;
    var last = 0;
    var acc = 0;

    var PLAY = "▶";
    var REPLAY = "↻";

    // Window the inner SVG onto frame `k` (0-based within the range).
    function show(k) {
      var idx = from + k;
      var c = idx % cols;
      var r = Math.floor(idx / cols);
      frame.setAttribute("viewBox", ox + c * sx + " " + (oy + r * sy) + " " + fw + " " + fh);
    }

    // The overlay glyph shows only while paused (▶) or ended (↻).
    function sync() {
      if (!btn) return;
      btn.style.display = playing ? "none" : "";
      btn.textContent = ended ? REPLAY : PLAY;
      btn.setAttribute("aria-label", ended ? "Replay" : "Play");
    }

    function tick(ts) {
      if (!playing) return;
      if (!last) last = ts;
      acc += ts - last;
      last = ts;
      // Advance whole frames; a backgrounded tab catches up at once.
      while (acc >= dt) {
        acc -= dt;
        i++;
        if (i >= count) {
          if (loop) {
            i = 0;
          } else {
            i = count - 1;
            show(i);
            playing = false;
            ended = true;
            raf = null;
            sync();
            return;
          }
        }
      }
      show(i);
      raf = requestAnimationFrame(tick);
    }

    function play() {
      if (ended) {
        i = 0;
        ended = false;
        show(0);
      }
      playing = true;
      last = 0;
      acc = 0;
      sync();
      raf = requestAnimationFrame(tick);
    }

    function pause() {
      playing = false;
      if (raf) cancelAnimationFrame(raf);
      raf = null;
      sync();
    }

    function toggle() {
      playing ? pause() : play();
    }

    g.addEventListener("click", toggle);

    show(0);
    sync();
    if (autoplay) play();
  }

  function boot() {
    var nodes = document.querySelectorAll("g.wdoc-dopesheet");
    for (var i = 0; i < nodes.length; i++) setup(nodes[i]);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
