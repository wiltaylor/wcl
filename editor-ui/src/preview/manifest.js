/** Whether the built site containing `href` (a page URL inside its output
    dir) lists `page` in the `_wdoc/pages.json` manifest full builds write.
    Not every view has a page per unit (a lesson is a training page; an
    :ai-only unit has none) — a missing/unreadable manifest reads as "no
    page". */
export async function builtPageExists(href, page) {
  const dir = href.slice(0, href.lastIndexOf('/') + 1);
  try {
    const manifest = await (await fetch(`${dir}_wdoc/pages.json`)).json();
    return (manifest.pages ?? []).includes(page);
  } catch {
    return false;
  }
}
