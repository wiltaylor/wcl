// wdoc terminal replay player.
//
// Each `[data-term-player]` element carries a `<script class="term-frames">`
// holding the recording as JSON (produced by the Rust renderer) and a
// `data-term-cells` id pointing at the SVG `<g class="term-cells">` group
// whose children we rebuild per frame. No framework, no dependencies.
(function () {
  "use strict";

  var SVGNS = "http://www.w3.org/2000/svg";
  var BASELINE = 0.78;
  var parser = new DOMParser();

  function esc(s) {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  // Build the inner SVG markup (background rects + glyph text + cursor)
  // for one frame. Coordinates are relative to the cell-group transform,
  // mirroring the Rust `runs_to_svg`.
  function frameMarkup(frame, d) {
    var bgs = "";
    var fgs = "";
    var rows = frame.rows || [];
    for (var r = 0; r < rows.length; r++) {
      var yRect = r * d.ch;
      var yText = r * d.ch + d.ch * BASELINE;
      var runs = rows[r];
      for (var i = 0; i < runs.length; i++) {
        var run = runs[i]; // [col, text, fg, bg, flags]
        var col = run[0], text = run[1], fg = run[2], bg = run[3], flags = run[4];
        // One background rect spans the whole coloured run.
        if (bg) {
          var w = text.length * d.cw;
          bgs += '<rect x="' + (col * d.cw).toFixed(2) + '" y="' + yRect.toFixed(2) +
            '" width="' + w.toFixed(2) + '" height="' + d.ch.toFixed(2) +
            '" fill="' + bg + '" shape-rendering="crispEdges"/>';
        }
        var hasDeco = flags & (4 | 8);
        var attrs = "";
        if (flags & 1) attrs += ' font-weight="bold"';
        if (flags & 2) attrs += ' font-style="italic"';
        var ul = flags & 4, st = flags & 8;
        if (ul && st) attrs += ' text-decoration="underline line-through"';
        else if (ul) attrs += ' text-decoration="underline"';
        else if (st) attrs += ' text-decoration="line-through"';
        if (flags & 16) attrs += ' class="term-blink"';
        // One centred glyph per cell — block/box chars tile seamlessly.
        for (var k = 0; k < text.length; k++) {
          var chr = text[k];
          if (chr === " " && !hasDeco) continue;
          var cx = (col + k) * d.cw + d.cw / 2;
          fgs += '<text x="' + cx.toFixed(2) + '" y="' + yText.toFixed(2) +
            '" text-anchor="middle" xml:space="preserve" fill="' + fg + '"' +
            attrs + ">" + esc(chr) + "</text>";
        }
      }
    }
    var cursor = "";
    if (frame.cur) {
      cursor = '<rect class="term-cursor" x="' + (frame.cur[0] * d.cw).toFixed(2) +
        '" y="' + (frame.cur[1] * d.ch).toFixed(2) + '" width="' + d.cw.toFixed(2) +
        '" height="' + d.ch.toFixed(2) + '"/>';
    }
    return bgs + fgs + cursor;
  }

  function paint(group, frame, d) {
    var doc = parser.parseFromString(
      '<svg xmlns="' + SVGNS + '">' + frameMarkup(frame, d) + "</svg>",
      "image/svg+xml"
    );
    var nodes = [];
    var src = doc.documentElement.childNodes;
    for (var i = 0; i < src.length; i++) nodes.push(document.importNode(src[i], true));
    group.replaceChildren.apply(group, nodes);
  }

  function init(root) {
    var script = root.querySelector("script.term-frames");
    if (!script) return;
    var data;
    try {
      data = JSON.parse(script.textContent);
    } catch (e) {
      return;
    }
    var frames = data.frames || [];
    if (!frames.length) return;
    var cellsId = root.getAttribute("data-term-cells");
    var group = cellsId && document.getElementById(cellsId);
    if (!group) return;

    var d = { cw: data.cw, ch: data.ch };
    var duration = frames[frames.length - 1].t || 0;
    var idx = 0;
    var playing = false;
    var ended = false;
    var speed = data.speed || 1; // playback rate from the WCL `speed` field
    var clock = 0; // virtual ms into the recording
    var raf = null;
    var lastTs = 0;

    // Big centred play button (overlay) + the chrome play/pause/replay
    // glyph next to the close ✕.
    var overlay = root.querySelector(".term-overlay-play");
    var chromeBtn = root.querySelector(".term-chrome-btn");

    function show(i) {
      idx = Math.max(0, Math.min(frames.length - 1, i));
      paint(group, frames[idx], d);
    }

    // Reflect the current state on both controls. We use glyphs that
    // have *text* (not emoji) presentation so they stay monochrome and
    // follow currentColor: ▶ play, two heavy bars for pause (U+23F8 ⏸
    // forces a colour emoji in Chrome), ↻ replay.
    var PLAY = "▶";
    var PAUSE = "❚❚";
    var REPLAY = "↻";
    function sync() {
      var glyph = ended ? REPLAY : playing ? PAUSE : PLAY;
      var label = ended ? "Replay" : playing ? "Pause" : "Play";
      if (chromeBtn) {
        chromeBtn.textContent = glyph;
        chromeBtn.setAttribute("aria-label", label);
      }
      if (overlay) {
        overlay.textContent = ended ? REPLAY : PLAY;
        overlay.setAttribute("aria-label", ended ? "Replay" : "Play");
        overlay.hidden = playing;
      }
    }

    // Advance to the frame whose timestamp is <= clock.
    function seekToClock() {
      var i = idx;
      while (i + 1 < frames.length && frames[i + 1].t <= clock) i++;
      while (i > 0 && frames[i].t > clock) i--;
      if (i !== idx) show(i);
    }

    function tick(ts) {
      if (!playing) return;
      if (!lastTs) lastTs = ts;
      clock += (ts - lastTs) * speed;
      lastTs = ts;
      if (clock >= duration) {
        if (data.loop) {
          clock = 0;
          idx = 0;
        } else {
          clock = duration;
          show(frames.length - 1);
          playing = false;
          ended = true;
          raf = null;
          sync();
          return;
        }
      }
      seekToClock();
      raf = requestAnimationFrame(tick);
    }

    function play() {
      // Restart from the top when replaying after the end.
      if (ended || (idx >= frames.length - 1 && !data.loop)) {
        clock = 0;
        show(0);
      }
      ended = false;
      playing = true;
      lastTs = 0;
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

    if (overlay) overlay.addEventListener("click", play);
    if (chromeBtn) chromeBtn.addEventListener("click", toggle);

    show(0);
    sync();
    if (data.autoplay) play();
  }

  function boot() {
    var roots = document.querySelectorAll("[data-term-player]");
    for (var i = 0; i < roots.length; i++) init(roots[i]);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
