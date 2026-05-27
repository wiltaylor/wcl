// wdoc map player: layer level-of-detail + pin popup cards.
//
// Each `.wdoc-map` is a `<g>` inside an interactive diagram `<svg>` (the
// pan/zoom player drives the camera). This add-on does two things:
//
//   1. Level-of-detail — when a map has more than one `.wdoc-map-layer`,
//      show the sharpest layer whose native resolution
//      (`data-native-width`) covers the on-screen pixels spanning the map
//      width at the current zoom; hide the rest. It reacts to the
//      `wdoc:camera` event the pan/zoom player dispatches on the `<svg>`.
//
//   2. Pin cards — clicking a `.wdoc-map-pin` opens its hidden
//      `.wdoc-map-card` (matched by `data-map-pin` / `data-map-card`) as a
//      popup anchored to the marker, kept glued to it as you pan/zoom.
//
// No framework, no dependencies. Loaded with `defer`; no-ops on a page
// with no maps.
(function () {
  "use strict";

  function cssEscape(s) {
    return String(s).replace(/["\\]/g, "\\$&");
  }

  function setup(map) {
    var svg = map.ownerSVGElement || (map.closest && map.closest("svg"));
    if (!svg) return;
    var viewport = svg.closest(".wdoc-diagram-viewport");
    var layers = map.querySelectorAll(".wdoc-map-layer");
    var mapWidth = parseFloat(map.getAttribute("data-map-width")) || 0;

    // ── Layer level-of-detail ───────────────────────────────────────
    function nativeWidth(layer) {
      var n = parseFloat(layer.getAttribute("data-native-width"));
      return isFinite(n) && n > 0 ? n : mapWidth;
    }
    function selectLayer(viewW) {
      if (layers.length <= 1) return; // single layer is always shown
      var clientW = svg.clientWidth || svg.getBoundingClientRect().width;
      if (!clientW || !viewW || !mapWidth) return;
      // On-screen pixels spanning the map's width at the current zoom.
      var needed = (mapWidth / viewW) * clientW;
      var chosen = null;
      var largest = layers[0];
      for (var i = 0; i < layers.length; i++) {
        var nw = nativeWidth(layers[i]);
        if (nw >= needed && (chosen === null || nw < nativeWidth(chosen))) {
          chosen = layers[i];
        }
        if (nw > nativeWidth(largest)) largest = layers[i];
      }
      if (chosen === null) chosen = largest; // none sharp enough → the best
      for (var k = 0; k < layers.length; k++) {
        layers[k].style.display = layers[k] === chosen ? "" : "none";
      }
    }

    // ── Pin popup cards ─────────────────────────────────────────────
    var openCard = null;
    var openPin = null;

    function cardFor(id) {
      if (!viewport || id == null) return null;
      return viewport.querySelector(
        '.wdoc-map-card[data-map-card="' + cssEscape(id) + '"]'
      );
    }
    function positionCard() {
      if (!openCard || !openPin || !viewport) return;
      var vp = viewport.getBoundingClientRect();
      var pr = openPin.getBoundingClientRect();
      var cx = pr.left + pr.width / 2 - vp.left;
      // Centre horizontally over the pin, clamped within the viewport.
      var cw = openCard.offsetWidth;
      var minL = cw / 2 + 2;
      var maxL = vp.width - cw / 2 - 2;
      if (maxL >= minL) cx = Math.max(minL, Math.min(maxL, cx));
      openCard.style.left = cx + "px";
      openCard.style.transform = "translateX(-50%)";
      // Place above the marker; flip below when there's no room.
      var ch = openCard.offsetHeight;
      var top = pr.top - vp.top - ch - 10;
      if (top < 4) top = pr.top - vp.top + pr.height + 10;
      openCard.style.top = top + "px";
    }
    function closeCard() {
      if (openCard) openCard.setAttribute("hidden", "");
      openCard = null;
      openPin = null;
    }
    function openFor(pin) {
      var card = cardFor(pin.getAttribute("data-map-pin"));
      if (!card) return;
      if (openCard === card) {
        closeCard(); // toggle off
        return;
      }
      closeCard();
      openCard = card;
      openPin = pin;
      card.removeAttribute("hidden");
      positionCard();
    }

    // A press with no drag on a pin is a tap. Stop the pointerdown from
    // reaching the svg pan handler so pressing a pin never starts a pan.
    var pins = map.querySelectorAll(".wdoc-map-pin");
    for (var p = 0; p < pins.length; p++) {
      (function (pin) {
        var sx = 0,
          sy = 0,
          moved = false;
        pin.addEventListener("pointerdown", function (e) {
          e.stopPropagation();
          sx = e.clientX;
          sy = e.clientY;
          moved = false;
        });
        pin.addEventListener("pointermove", function (e) {
          if (Math.abs(e.clientX - sx) > 4 || Math.abs(e.clientY - sy) > 4) {
            moved = true;
          }
        });
        pin.addEventListener("pointerup", function (e) {
          e.stopPropagation();
          if (!moved) openFor(pin);
        });
      })(pins[p]);
    }

    // Close on the card's ✕, or on a click outside any card / pin.
    if (viewport) {
      viewport.addEventListener("click", function (e) {
        var t = e.target;
        if (!t || !t.closest) return;
        if (t.closest(".wdoc-map-card-close")) {
          closeCard();
          return;
        }
        if (openCard && !t.closest(".wdoc-map-card") && !t.closest(".wdoc-map-pin")) {
          closeCard();
        }
      });
    }

    // React to the camera: re-pick the layer + keep the open card glued to
    // its pin. (The event may have fired before we attached, so we also do
    // an initial pass below.)
    svg.addEventListener("wdoc:camera", function (e) {
      selectLayer(e.detail && e.detail.w);
      positionCard();
    });

    var vb = svg.viewBox && svg.viewBox.baseVal;
    selectLayer(vb && vb.width ? vb.width : mapWidth);
  }

  function init() {
    var maps = document.querySelectorAll(".wdoc-map");
    for (var i = 0; i < maps.length; i++) setup(maps[i]);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
