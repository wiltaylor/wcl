// wdoc video player.
//
// Every `<div class="wdoc-video">` renders as a lightweight click-to-play
// facade — a poster thumbnail plus a play button — so no video loads until
// the reader clicks. The facade carries the playable / embed URL in
// `data-src` and the player kind in `data-kind`:
//   local   → a native <video controls autoplay>
//   youtube → an <iframe> onto the YouTube embed URL (autoplay=1)
//   vimeo   → an <iframe> onto the Vimeo player URL (autoplay=1)
//   generic → an <iframe> onto the URL verbatim
// On click we replace the facade's contents with the real element once.
// No framework, no dependencies. Loaded with `defer`, so the DOM is ready.
(function () {
  "use strict";

  function activate(box) {
    if (box.getAttribute("data-active") === "1") return;
    var src = box.getAttribute("data-src");
    if (!src) return;
    var kind = box.getAttribute("data-kind") || "generic";
    var title = box.getAttribute("aria-label") || "";

    var el;
    if (kind === "local") {
      el = document.createElement("video");
      el.setAttribute("controls", "");
      el.setAttribute("autoplay", "");
      el.setAttribute("playsinline", "");
      el.setAttribute("src", src);
    } else {
      el = document.createElement("iframe");
      el.setAttribute("src", src);
      el.setAttribute("allow", "autoplay; encrypted-media; fullscreen; picture-in-picture");
      el.setAttribute("allowfullscreen", "");
      el.setAttribute("frameborder", "0");
      if (title) el.setAttribute("title", title);
    }

    box.innerHTML = "";
    box.appendChild(el);
    box.setAttribute("data-active", "1");
    box.style.cursor = "default";
  }

  function boot() {
    var nodes = document.querySelectorAll("div.wdoc-video");
    for (var i = 0; i < nodes.length; i++) {
      (function (box) {
        box.addEventListener("click", function () {
          activate(box);
        });
      })(nodes[i]);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();
