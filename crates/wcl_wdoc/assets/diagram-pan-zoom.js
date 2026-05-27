// wdoc interactive diagram pan + zoom.
//
// Every `.wdoc-diagram-viewport` wraps an `<svg data-pan-zoom>` whose
// `data-base-viewbox` is the fitted view the Rust renderer computed and
// whose `data-zoom-min` / `data-zoom-max` / `data-pan-margin` are the
// authored limits. The camera is the SVG `viewBox`: zoom shrinks/grows
// it about a focal point, pan shifts its top-left. No framework, no
// dependencies. Loaded with `defer`, so the DOM is ready when it runs.
(function () {
  "use strict";

  function clamp(v, lo, hi) {
    return v < lo ? lo : v > hi ? hi : v;
  }

  function num(el, name, fallback) {
    var v = parseFloat(el.getAttribute(name));
    return isFinite(v) ? v : fallback;
  }

  function setup(viewport) {
    var svg = viewport.querySelector("svg[data-pan-zoom]");
    if (!svg) return;
    var base = (svg.getAttribute("data-base-viewbox") || "")
      .trim()
      .split(/\s+/)
      .map(parseFloat);
    if (base.length !== 4 || base.some(function (n) { return !isFinite(n); })) return;

    var bx = base[0], by = base[1], bw = base[2], bh = base[3];
    var zoomMin = num(svg, "data-zoom-min", 1);
    var zoomMax = num(svg, "data-zoom-max", 4);
    var margin = num(svg, "data-pan-margin", 0);

    // Camera state: zoom factor + top-left corner (in diagram coords).
    var z = 1, x = bx, y = by;

    // Clamp one axis. When zoomed out to (or past) the fitted view the
    // whole content is visible, so there's nothing to pan — centre it.
    // When zoomed in, allow generous overscroll so the view never feels
    // jammed against the content edges: half a viewport of slack on each
    // side, plus the author's pan_margin (in diagram units).
    function clampAxis(pos, size, originBase, extent) {
      if (size >= extent) return originBase + extent / 2 - size / 2;
      var slack = margin + size * 0.5;
      var lo = originBase - slack;
      var hi = originBase + extent + slack - size;
      return clamp(pos, lo, hi);
    }

    function apply() {
      z = clamp(z, zoomMin, zoomMax);
      var w = bw / z, h = bh / z;
      x = clampAxis(x, w, bx, bw);
      y = clampAxis(y, h, by, bh);
      svg.setAttribute("viewBox", x + " " + y + " " + w + " " + h);
      // Broadcast the camera so add-ons (e.g. the map player's layer
      // level-of-detail + card positioning) can react. No-op when nothing
      // is listening.
      svg.dispatchEvent(
        new CustomEvent("wdoc:camera", { detail: { x: x, y: y, w: w, h: h, z: z } })
      );
    }

    // Zoom about a focal point given as fractions (fx, fy) of the
    // current view, keeping that point fixed on screen.
    function zoomAt(factor, fx, fy) {
      var w = bw / z, h = bh / z;
      var px = x + fx * w, py = y + fy * h;
      z = clamp(z * factor, zoomMin, zoomMax);
      var nw = bw / z, nh = bh / z;
      x = px - fx * nw;
      y = py - fy * nh;
      apply();
    }

    svg.addEventListener(
      "wheel",
      function (e) {
        e.preventDefault();
        var rect = svg.getBoundingClientRect();
        if (!rect.width || !rect.height) return;
        var fx = (e.clientX - rect.left) / rect.width;
        var fy = (e.clientY - rect.top) / rect.height;
        zoomAt(e.deltaY < 0 ? 1.1 : 1 / 1.1, fx, fy);
      },
      { passive: false }
    );

    var dragging = false, lastX = 0, lastY = 0;
    svg.addEventListener("pointerdown", function (e) {
      dragging = true;
      lastX = e.clientX;
      lastY = e.clientY;
      viewport.classList.add("panning");
      if (svg.setPointerCapture) svg.setPointerCapture(e.pointerId);
    });
    svg.addEventListener("pointermove", function (e) {
      if (!dragging) return;
      var rect = svg.getBoundingClientRect();
      if (!rect.width || !rect.height) return;
      var w = bw / z, h = bh / z;
      x -= (e.clientX - lastX) * (w / rect.width);
      y -= (e.clientY - lastY) * (h / rect.height);
      lastX = e.clientX;
      lastY = e.clientY;
      apply();
    });
    function endDrag(e) {
      if (!dragging) return;
      dragging = false;
      viewport.classList.remove("panning");
      if (svg.releasePointerCapture && e.pointerId != null) {
        try { svg.releasePointerCapture(e.pointerId); } catch (_) {}
      }
    }
    svg.addEventListener("pointerup", endDrag);
    svg.addEventListener("pointercancel", endDrag);

    var buttons = viewport.querySelectorAll(".wdoc-diagram-controls [data-zoom]");
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].addEventListener("click", function (e) {
        var kind = e.currentTarget.getAttribute("data-zoom");
        if (kind === "in") zoomAt(1.2, 0.5, 0.5);
        else if (kind === "out") zoomAt(1 / 1.2, 0.5, 0.5);
        else {
          z = 1;
          x = bx;
          y = by;
          apply();
        }
      });
    }

    apply();
  }

  function init() {
    var nodes = document.querySelectorAll(".wdoc-diagram-viewport");
    for (var i = 0; i < nodes.length; i++) setup(nodes[i]);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
