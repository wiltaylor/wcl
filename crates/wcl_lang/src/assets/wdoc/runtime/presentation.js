(function() {
    function go(selector) {
        var link = document.querySelector(selector);
        if (!link) return false;
        window.location.href = link.getAttribute('href');
        return true;
    }
    document.addEventListener('keydown', function(event) {
        if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
        var tag = event.target && event.target.tagName ? event.target.tagName.toLowerCase() : '';
        if (tag === 'input' || tag === 'textarea' || tag === 'select') return;
        var handled = false;
        if (event.key === 'ArrowRight' || event.key === 'PageDown' || event.key === ' ') {
            handled = go('[data-wdoc-slide-right]');
        } else if (event.key === 'ArrowLeft' || event.key === 'Backspace') {
            handled = go('[data-wdoc-slide-left]');
        } else if (event.key === 'ArrowDown') {
            handled = go('[data-wdoc-slide-down]');
        } else if (event.key === 'ArrowUp' || event.key === 'PageUp') {
            handled = go('[data-wdoc-slide-up]');
        }
        if (handled) {
            event.preventDefault();
        }
    });
})();
