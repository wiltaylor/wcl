(function() {
    // Determine initial theme: saved preference > system preference > light
    function getPreferred() {
        var saved = localStorage.getItem('wdoc-theme');
        if (saved === 'dark' || saved === 'light') return saved;
        if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) return 'dark';
        return 'light';
    }

    function applyTheme(theme) {
        document.documentElement.setAttribute('data-theme', theme);
        var light = document.getElementById('hljs-light');
        var dark = document.getElementById('hljs-dark');
        if (light && dark) {
            light.disabled = (theme === 'dark');
            dark.disabled = (theme !== 'dark');
        }
        var icon = document.getElementById('wdoc-theme-icon');
        if (icon) icon.textContent = (theme === 'dark') ? '\u{2600}\u{FE0F}' : '\u{1F319}';
        localStorage.setItem('wdoc-theme', theme);
    }

    // Apply immediately (before DOM ready) to prevent flash
    applyTheme(getPreferred());

    document.addEventListener('DOMContentLoaded', function() {
        // highlight.js init
        if (typeof hljs !== 'undefined') {
            if (typeof hljsDefineWcl !== 'undefined') hljs.registerLanguage('wcl', hljsDefineWcl);
            hljs.highlightAll();
        }

        // Toggle button
        var toggle = document.getElementById('wdoc-theme-toggle');
        if (toggle) {
            toggle.addEventListener('click', function() {
                var current = document.documentElement.getAttribute('data-theme') || 'light';
                applyTheme(current === 'dark' ? 'light' : 'dark');
                // Re-highlight with new theme
                if (typeof hljs !== 'undefined') {
                    document.querySelectorAll('pre code').forEach(function(el) {
                        el.removeAttribute('data-highlighted');
                        hljs.highlightElement(el);
                    });
                }
            });
        }

        // Listen for system theme changes
        if (window.matchMedia) {
            window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function(e) {
                if (!localStorage.getItem('wdoc-theme')) {
                    applyTheme(e.matches ? 'dark' : 'light');
                }
            });
        }
    });
})();
