// wdoc presentation (deck) player.
//
// The deck is one HTML file: a `.deck` holding `.deck-section`s (columns),
// each holding `.deck-slide`s (rows). We show exactly one slide at a time
// (the `.active` one) and navigate the 2-D grid by keyboard:
//
//   ← / →            move between sections (land on the section's top slide)
//   ↑ / ↓            move between slides within the current section
//   Space / PageDown step forward: reveal the next `.wdoc-fragment`, then
//                    advance to the next slide in flow
//   ⇧Space / PageUp  step backward
//   Home / End       first / last slide
//   s                toggle the speaker-notes overlay
//   f                toggle fullscreen
//
// Arrow keys show the target slide with all its fragments revealed; the
// Space path reveals fragments one at a time. No framework, no deps.
// Loaded with `defer`, so the DOM is ready.
(function () {
  "use strict";

  var deck = document.querySelector(".deck");
  if (!deck) return;

  // sections[i] = array of that section's slide elements (in order).
  var sections = [];
  var sectionEls = deck.querySelectorAll(".deck-section");
  for (var i = 0; i < sectionEls.length; i++) {
    var slideEls = sectionEls[i].querySelectorAll(".deck-slide");
    var slides = [];
    for (var j = 0; j < slideEls.length; j++) slides.push(slideEls[j]);
    if (slides.length) sections.push(slides);
  }
  if (!sections.length) return;

  var bar = document.querySelector(".deck-progress-bar");
  var counter = document.querySelector(".deck-counter");
  var hintUp = document.querySelector(".deck-hint-up");
  var hintDown = document.querySelector(".deck-hint-down");
  var hintLeft = document.querySelector(".deck-hint-left");
  var hintRight = document.querySelector(".deck-hint-right");

  var total = 0;
  for (var s = 0; s < sections.length; s++) total += sections[s].length;

  var si = 0; // current section
  var pi = 0; // current slide within the section
  var fi = 0; // number of fragments revealed on the current slide

  function frags(slide) {
    return slide ? slide.querySelectorAll(".wdoc-fragment") : [];
  }

  function clamp(v, lo, hi) {
    return v < lo ? lo : v > hi ? hi : v;
  }

  function globalIndex() {
    var n = 0;
    for (var k = 0; k < si; k++) n += sections[k].length;
    return n + pi;
  }

  function setHint(el, on) {
    if (el) el.classList.toggle("on", !!on);
  }

  function render() {
    for (var a = 0; a < sections.length; a++) {
      for (var b = 0; b < sections[a].length; b++) {
        sections[a][b].classList.toggle("active", a === si && b === pi);
      }
    }
    var cur = sections[si][pi];
    var fr = frags(cur);
    for (var f = 0; f < fr.length; f++) {
      fr[f].classList.toggle("revealed", f < fi);
    }

    var gi = globalIndex();
    if (counter) counter.textContent = gi + 1 + " / " + total;
    if (bar) bar.style.width = (total > 1 ? (gi / (total - 1)) * 100 : 100) + "%";
    setHint(hintUp, pi > 0);
    setHint(hintDown, pi < sections[si].length - 1);
    setHint(hintLeft, si > 0);
    setHint(hintRight, si < sections.length - 1);

    var hash = "#/" + si + "/" + pi;
    if (location.hash !== hash) {
      try {
        history.replaceState(null, "", hash);
      } catch (e) {
        location.hash = hash;
      }
    }
  }

  // Jump straight to a slide; `revealAll` shows every fragment (arrow nav),
  // otherwise none are revealed.
  function go(nsi, npi, revealAll) {
    si = clamp(nsi, 0, sections.length - 1);
    pi = clamp(npi, 0, sections[si].length - 1);
    fi = revealAll ? frags(sections[si][pi]).length : 0;
    render();
  }

  // Step forward: reveal the next fragment, else advance one slide in flow
  // (down the column, then on to the next section's first slide).
  function next() {
    var fr = frags(sections[si][pi]);
    if (fi < fr.length) {
      fi++;
      render();
      return;
    }
    if (pi < sections[si].length - 1) {
      go(si, pi + 1, false);
    } else if (si < sections.length - 1) {
      go(si + 1, 0, false);
    }
  }

  // Step backward: hide the last fragment, else retreat one slide in flow
  // (fully revealed, so you see where you came from).
  function prev() {
    if (fi > 0) {
      fi--;
      render();
      return;
    }
    if (pi > 0) {
      go(si, pi - 1, true);
    } else if (si > 0) {
      go(si - 1, sections[si - 1].length - 1, true);
    }
  }

  function lastSlide() {
    var ls = sections.length - 1;
    go(ls, sections[ls].length - 1, true);
  }

  function fromHash() {
    var m = /^#\/(\d+)\/(\d+)$/.exec(location.hash || "");
    if (m) go(parseInt(m[1], 10), parseInt(m[2], 10), true);
    else render();
  }

  document.addEventListener("keydown", function (e) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    var k = e.key;
    if (k === "ArrowRight") go(si + 1, 0, true);
    else if (k === "ArrowLeft") go(si - 1, 0, true);
    else if (k === "ArrowDown") go(si, pi + 1, true);
    else if (k === "ArrowUp") go(si, pi - 1, true);
    else if (k === "PageDown" || (k === " " && !e.shiftKey)) next();
    else if (k === "PageUp" || (k === " " && e.shiftKey)) prev();
    else if (k === "Home") go(0, 0, true);
    else if (k === "End") lastSlide();
    else if (k === "s" || k === "S") {
      deck.classList.toggle("show-notes");
      return;
    } else if (k === "f" || k === "F") {
      if (document.fullscreenElement) document.exitFullscreen();
      else if (document.documentElement.requestFullscreen)
        document.documentElement.requestFullscreen();
      return;
    } else return;
    e.preventDefault();
  });

  window.addEventListener("hashchange", fromHash);
  fromHash();
})();
