// Register the WCL grammar (loaded from extras/highlightjs/wcl.js via additional-js)
// and re-highlight WCL code blocks that mdbook's built-in hljs pass already
// processed before the 'wcl' language was available.
hljs.registerLanguage('wcl', hljsDefineWcl);

document.querySelectorAll('code.language-wcl').forEach(function (block) {
  var result = hljs.highlight(block.textContent, { language: 'wcl' });
  block.innerHTML = result.value;
  block.classList.add('hljs');
});
