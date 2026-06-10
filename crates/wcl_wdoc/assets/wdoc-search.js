// wdoc site search. Attaches to every `.wdoc-search` box (rendered by a
// template when the site sets `search = true`), lazily fetches the
// build-time index at `_wdoc/search-index.json` (a sibling of every
// page, so a relative path works when the site is served), and renders
// ranked page links as you type.
(function () {
  'use strict';

  var INDEX_URL = '_wdoc/search-index.json';
  var MAX_RESULTS = 8;

  function tokens(q) {
    return q.toLowerCase().split(/\s+/).filter(Boolean);
  }

  // A page matches when every term occurs in its title or text. Title
  // hits dominate the score; earlier text hits beat later ones a little.
  function score(entry, terms) {
    var total = 0;
    for (var i = 0; i < terms.length; i++) {
      var t = terms[i];
      var inTitle = entry.titleLower.indexOf(t) !== -1;
      var at = entry.textLower.indexOf(t);
      if (!inTitle && at === -1) return 0;
      total += inTitle ? 100 : 0;
      if (at !== -1) total += 10 + 5 / (1 + at / 500);
    }
    return total;
  }

  // A short context window around the first occurrence of the first
  // matching term.
  function snippet(entry, terms) {
    var at = -1;
    for (var i = 0; i < terms.length && at === -1; i++) {
      at = entry.textLower.indexOf(terms[i]);
    }
    if (at === -1) return '';
    var start = Math.max(0, at - 40);
    var end = Math.min(entry.text.length, at + 80);
    return (
      (start > 0 ? '…' : '') +
      entry.text.slice(start, end).trim() +
      (end < entry.text.length ? '…' : '')
    );
  }

  function attach(box) {
    var input = box.querySelector('.wdoc-search-input');
    var results = box.querySelector('.wdoc-search-results');
    if (!input || !results) return;

    var index = null;
    var loading = null;
    function ensureIndex() {
      if (index) return Promise.resolve(index);
      if (!loading) {
        loading = fetch(INDEX_URL)
          .then(function (r) { return r.json(); })
          .then(function (entries) {
            entries.forEach(function (e) {
              e.titleLower = e.title.toLowerCase();
              e.textLower = e.text.toLowerCase();
            });
            index = entries;
            return index;
          });
      }
      return loading;
    }

    function clear() {
      results.innerHTML = '';
      results.classList.remove('open');
    }

    function render(query) {
      var terms = tokens(query);
      if (!terms.length) return clear();
      ensureIndex().then(function (entries) {
        var hits = entries
          .map(function (e) { return { e: e, s: score(e, terms) }; })
          .filter(function (h) { return h.s > 0; })
          .sort(function (a, b) { return b.s - a.s; })
          .slice(0, MAX_RESULTS);
        results.innerHTML = '';
        if (!hits.length) {
          var empty = document.createElement('div');
          empty.className = 'wdoc-search-empty';
          empty.textContent = 'No matches';
          results.appendChild(empty);
        }
        hits.forEach(function (h) {
          var a = document.createElement('a');
          a.href = h.e.href;
          a.className = 'wdoc-search-hit';
          var t = document.createElement('span');
          t.className = 'wdoc-search-hit-title';
          t.textContent = h.e.title;
          a.appendChild(t);
          var s = snippet(h.e, terms);
          if (s) {
            var sn = document.createElement('span');
            sn.className = 'wdoc-search-hit-snippet';
            sn.textContent = s;
            a.appendChild(sn);
          }
          results.appendChild(a);
        });
        results.classList.add('open');
      });
    }

    input.addEventListener('input', function () { render(input.value); });
    input.addEventListener('keydown', function (ev) {
      if (ev.key === 'Escape') {
        input.value = '';
        clear();
        input.blur();
      } else if (ev.key === 'Enter') {
        var first = results.querySelector('a.wdoc-search-hit');
        if (first) window.location.href = first.getAttribute('href');
      }
    });
    document.addEventListener('click', function (ev) {
      if (!box.contains(ev.target)) clear();
    });
  }

  document.querySelectorAll('.wdoc-search').forEach(attach);
})();
