(function () {
  var sections = Array.from(document.querySelectorAll("section[id]"));
  var navLinks = document.querySelectorAll(".section-nav-link");
  var nav = document.getElementById("section-nav");

  if (!sections.length || !navLinks.length) return;

  var current = 0;
  var animating = false;
  var DURATION = 400;
  var COOLDOWN = 900;
  var lastTransition = 0;

  function checkShortContent() {
    // No longer needed with block layout
  }

  var upSvg = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="18 15 12 9 6 15"/></svg>';
  var downSvg = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>';

  function init() {
    sections.forEach(function (s, i) {
      s.style.position = "relative";

      // Add up arrow (except first section)
      if (i > 0) {
        var up = document.createElement("button");
        up.className = "section-arrow section-arrow-up";
        up.setAttribute("aria-label", "Previous section");
        up.innerHTML = upSvg;
        up.addEventListener("click", function () { goTo(i - 1); });
        s.insertBefore(up, s.firstChild);
      }

      // Add down arrow (except last section)
      if (i < sections.length - 1) {
        var down = document.createElement("button");
        down.className = "section-arrow section-arrow-down";
        down.setAttribute("aria-label", "Next section");
        down.innerHTML = downSvg;
        down.addEventListener("click", function () { goTo(i + 1); });
        s.appendChild(down);
      }

      if (i !== 0) {
        s.style.display = "none";
        s.style.opacity = "0";
      } else {
        s.style.display = "";
        s.style.opacity = "1";
      }
    });
    checkShortContent();
    updateNav();
    window.addEventListener("resize", checkShortContent);
  }

  function updateNav() {
    var id = sections[current].id;
    navLinks.forEach(function (link) {
      link.classList.toggle("active", link.dataset.section === id);
    });
  }

  function goTo(index) {
    if (index < 0 || index >= sections.length || index === current || animating) return;
    if (Date.now() - lastTransition < COOLDOWN) return;
    animating = true;

    var goingDown = index > current;
    var from = sections[current];
    var to = sections[index];

    // Prep target
    to.style.display = "";
    to.style.transition = "none";
    to.style.transform = "translateY(" + (goingDown ? 60 : -60) + "px)";
    to.style.opacity = "0";
    to.scrollTop = 0;
    void to.offsetHeight;

    // Animate out
    from.style.transition = "transform " + DURATION + "ms ease, opacity " + DURATION + "ms ease";
    from.style.transform = "translateY(" + (goingDown ? -60 : 60) + "px)";
    from.style.opacity = "0";

    // Animate in
    to.style.transition = "transform " + DURATION + "ms ease, opacity " + DURATION + "ms ease";
    to.style.transform = "translateY(0)";
    to.style.opacity = "1";

    setTimeout(function () {
      from.style.display = "none";
      from.style.transition = "";
      from.style.transform = "";
      from.style.opacity = "";

      to.style.transition = "";
      to.style.transform = "";

      current = index;
      updateNav();
      animating = false;
      lastTransition = Date.now();
    }, DURATION);
  }

  // Check if a section can scroll internally
  function canScrollDown(el) {
    return el.scrollTop + el.clientHeight < el.scrollHeight - 2;
  }

  function canScrollUp(el) {
    return el.scrollTop > 2;
  }

  // Wheel — scroll internally first, then switch at edge
  document.addEventListener("wheel", function (e) {
    if (animating) { e.preventDefault(); return; }

    var sec = sections[current];
    var down = e.deltaY > 0;

    if (down && canScrollDown(sec)) return;
    if (!down && canScrollUp(sec)) return;

    e.preventDefault();
    if (down) goTo(current + 1);
    else goTo(current - 1);
  }, { passive: false });

  // Keyboard
  document.addEventListener("keydown", function (e) {
    if (e.key === "ArrowDown" || e.key === "PageDown") {
      var sec = sections[current];
      if (canScrollDown(sec)) return;
      e.preventDefault();
      goTo(current + 1);
    } else if (e.key === "ArrowUp" || e.key === "PageUp") {
      var sec2 = sections[current];
      if (canScrollUp(sec2)) return;
      e.preventDefault();
      goTo(current - 1);
    } else if (e.key === "Home") {
      e.preventDefault();
      goTo(0);
    } else if (e.key === "End") {
      e.preventDefault();
      goTo(sections.length - 1);
    }
  });

  // Touch
  var touchStartY = 0;
  var touchMoved = false;
  document.addEventListener("touchstart", function (e) {
    touchStartY = e.touches[0].clientY;
    touchMoved = false;
  }, { passive: true });

  document.addEventListener("touchmove", function () {
    touchMoved = true;
  }, { passive: true });

  document.addEventListener("touchend", function (e) {
    if (!touchMoved) return;
    var delta = touchStartY - e.changedTouches[0].clientY;
    var sec = sections[current];

    if (delta > 50 && !canScrollDown(sec)) {
      goTo(current + 1);
    } else if (delta < -50 && !canScrollUp(sec)) {
      goTo(current - 1);
    }
  }, { passive: true });


  // Nav clicks
  navLinks.forEach(function (link) {
    link.addEventListener("click", function (e) {
      e.preventDefault();
      var targetId = this.dataset.section;
      for (var i = 0; i < sections.length; i++) {
        if (sections[i].id === targetId) {
          goTo(i);
          break;
        }
      }
    });
  });

  init();
})();
