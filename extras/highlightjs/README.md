# WCL highlight.js Grammar

Syntax highlighting definition for [highlight.js](https://highlightjs.org/), the default code highlighter used by mdbook.

## Usage with mdbook

1. Copy `wcl.js` to your mdbook theme directory
2. Register it in your `book.toml`:

```toml
[output.html]
additional-js = ["theme/wcl.js"]
```

3. In your `theme/index.hbs`, register the language:

```html
<script>
hljs.registerLanguage('wcl', window.hljsDefineWcl);
</script>
```

## Usage standalone

```javascript
import hljs from 'highlight.js/lib/core';
import wcl from './wcl.js';

hljs.registerLanguage('wcl', wcl);
hljs.highlightAll();
```
