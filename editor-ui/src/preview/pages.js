/* Preview page addressing: turning a build's location into the URL of one
   page inside it, and answering whether the build actually contains that
   page. Pure and side-effect free apart from the manifest fetch — every
   preview host reaches a page through here rather than doing the string
   surgery itself. */

/** The output directory a built page URL lives in, trailing slash kept. */
export function dirOf(href) {
  if (!href) return null;
  return href.slice(0, href.lastIndexOf('/') + 1);
}

/** The URL of `page` inside the build `href` points into — every built
    site renders one `<page>.html` per page name, beside the index. */
export function pageHref(href, page) {
  const dir = dirOf(href);
  return dir && page ? `${dir}${page}.html` : null;
}

/** Whether the built site containing `href` (a page URL inside its output
    dir) lists `page` in the `_wdoc/pages.json` manifest full builds write.
    Not every view has a page per unit (a lesson is a training page; an
    :ai-only unit has none) — a missing/unreadable manifest reads as "no
    page". */
export async function builtPageExists(href, page) {
  const dir = dirOf(href);
  if (!dir || !page) return false;
  try {
    const manifest = await (await fetch(`${dir}_wdoc/pages.json`)).json();
    return (manifest.pages ?? []).includes(page);
  } catch {
    return false;
  }
}
