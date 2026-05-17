use std::path::PathBuf;

pub const CODECS_LIBRARY_WCL: &str = include_str!("std/codecs.wcl");
pub const HTML_LIBRARY_WCL: &str = include_str!("std/html.wcl");
pub const SVG_LIBRARY_WCL: &str = include_str!("std/svg.wcl");
pub const CSS_LIBRARY_WCL: &str = include_str!("std/css.wcl");

pub fn install_codecs_library(force: bool) -> Result<Vec<PathBuf>, String> {
    let lib_dir = crate::library::user_library_dir();
    std::fs::create_dir_all(&lib_dir)
        .map_err(|e| format!("failed to create library dir {}: {e}", lib_dir.display()))?;
    let libraries = [
        ("codecs.wcl", CODECS_LIBRARY_WCL),
        ("html.wcl", HTML_LIBRARY_WCL),
        ("svg.wcl", SVG_LIBRARY_WCL),
        ("css.wcl", CSS_LIBRARY_WCL),
    ];
    for (name, _) in libraries {
        let target = lib_dir.join(name);
        if target.exists() && !force {
            return Err(format!(
                "{} already exists (use --force to overwrite)",
                target.display()
            ));
        }
    }
    let mut installed = Vec::new();
    for (name, content) in libraries {
        let target = lib_dir.join(name);
        std::fs::write(&target, content)
            .map_err(|e| format!("failed to write {}: {e}", target.display()))?;
        installed.push(target);
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use crate::{parse, ParseOptions};

    // Downloaded from the MDN browser-compat-data repository:
    // https://github.com/mdn/browser-compat-data/tree/main/html/elements
    const MDN_HTML_ELEMENTS: &[&str] = &[
        "a",
        "abbr",
        "acronym",
        "address",
        "area",
        "article",
        "aside",
        "audio",
        "b",
        "base",
        "bdi",
        "bdo",
        "big",
        "blockquote",
        "body",
        "br",
        "button",
        "canvas",
        "caption",
        "center",
        "cite",
        "code",
        "col",
        "colgroup",
        "data",
        "datalist",
        "dd",
        "del",
        "details",
        "dfn",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "em",
        "embed",
        "fencedframe",
        "fieldset",
        "figcaption",
        "figure",
        "font",
        "footer",
        "form",
        "frame",
        "frameset",
        "geolocation",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hgroup",
        "hr",
        "html",
        "i",
        "iframe",
        "img",
        "input",
        "ins",
        "kbd",
        "label",
        "legend",
        "li",
        "link",
        "main",
        "map",
        "mark",
        "marquee",
        "menu",
        "meta",
        "meter",
        "nav",
        "nobr",
        "noembed",
        "noframes",
        "noscript",
        "object",
        "ol",
        "optgroup",
        "option",
        "output",
        "p",
        "param",
        "picture",
        "plaintext",
        "pre",
        "progress",
        "q",
        "rb",
        "rp",
        "rt",
        "rtc",
        "ruby",
        "s",
        "samp",
        "script",
        "search",
        "section",
        "select",
        "selectedcontent",
        "slot",
        "small",
        "source",
        "span",
        "strike",
        "strong",
        "style",
        "sub",
        "summary",
        "sup",
        "table",
        "tbody",
        "td",
        "template",
        "textarea",
        "tfoot",
        "th",
        "thead",
        "time",
        "title",
        "tr",
        "track",
        "tt",
        "u",
        "ul",
        "var",
        "video",
        "wbr",
        "xmp",
    ];

    // Downloaded from the MDN browser-compat-data repository:
    // https://github.com/mdn/browser-compat-data/tree/main/svg/elements
    const MDN_SVG_ELEMENTS: &[&str] = &[
        "a",
        "animate",
        "animateMotion",
        "animateTransform",
        "circle",
        "clipPath",
        "defs",
        "desc",
        "ellipse",
        "feBlend",
        "feColorMatrix",
        "feComponentTransfer",
        "feComposite",
        "feConvolveMatrix",
        "feDiffuseLighting",
        "feDisplacementMap",
        "feDistantLight",
        "feDropShadow",
        "feFlood",
        "feFuncA",
        "feFuncB",
        "feFuncG",
        "feFuncR",
        "feGaussianBlur",
        "feImage",
        "feMerge",
        "feMergeNode",
        "feMorphology",
        "feOffset",
        "fePointLight",
        "feSpecularLighting",
        "feSpotLight",
        "feTile",
        "feTurbulence",
        "filter",
        "foreignObject",
        "g",
        "image",
        "line",
        "linearGradient",
        "marker",
        "mask",
        "metadata",
        "mpath",
        "path",
        "pattern",
        "polygon",
        "polyline",
        "radialGradient",
        "rect",
        "script",
        "set",
        "stop",
        "style",
        "svg",
        "switch",
        "symbol",
        "text",
        "textPath",
        "title",
        "tspan",
        "use",
        "view",
    ];

    #[test]
    fn bundled_standard_libraries_parse() {
        for (name, source) in [
            ("codecs.wcl", super::CODECS_LIBRARY_WCL),
            ("html.wcl", super::HTML_LIBRARY_WCL),
            ("svg.wcl", super::SVG_LIBRARY_WCL),
            ("css.wcl", super::CSS_LIBRARY_WCL),
        ] {
            let doc = parse(source, ParseOptions::default());
            assert!(
                !doc.has_errors(),
                "{name} should parse without diagnostics: {:?}",
                doc.errors()
            );
        }
    }

    #[test]
    fn html_and_svg_libraries_cover_mdn_element_reference() {
        assert_schema_source_covers("html.wcl", super::HTML_LIBRARY_WCL, MDN_HTML_ELEMENTS);
        assert_schema_source_covers("svg.wcl", super::SVG_LIBRARY_WCL, MDN_SVG_ELEMENTS);
    }

    #[test]
    fn css_library_covers_mdn_reference_data() {
        assert_wcl_list_count("mdn_properties", 663);
        assert_wcl_list_count("mdn_at_rules", 19);
        assert_wcl_list_count("mdn_selectors", 144);
        assert_wcl_list_count("mdn_functions", 105);
        assert_wcl_list_count("mdn_syntaxes", 378);
        assert_wcl_list_count("mdn_types", 39);
        assert_wcl_list_count("mdn_units", 30);

        for expected in [
            "\"anchor-name\"",
            "\"font-family\"",
            "\"view-transition-name\"",
            "\"@media\"",
            "\"@keyframes\"",
            "\"::backdrop\"",
            "\":has()\"",
            "\"color-mix()\"",
            "\"length-percentage\"",
            "\"dppx\"",
        ] {
            assert!(
                super::CSS_LIBRARY_WCL.contains(expected),
                "css.wcl should include MDN CSS reference item {expected}"
            );
        }
    }

    fn assert_schema_source_covers(name: &str, source: &str, elements: &[&str]) {
        let missing = elements
            .iter()
            .filter(|element| !source.contains(&format!("schema \"{element}\"")))
            .copied()
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "{name} is missing element schemas from MDN browser-compat-data: {missing:?}"
        );
    }

    fn assert_wcl_list_count(name: &str, expected: usize) {
        let needle = format!("export let {name} = [");
        let start = super::CSS_LIBRARY_WCL
            .find(&needle)
            .unwrap_or_else(|| panic!("missing {name} list"));
        let list = &super::CSS_LIBRARY_WCL[start + needle.len()..];
        let end = list
            .find("\n    ]")
            .unwrap_or_else(|| panic!("unterminated {name} list"));
        let count = list[..end]
            .lines()
            .filter(|line| line.trim_start().starts_with('"'))
            .count();
        assert_eq!(count, expected, "{name} should match MDN data count");
    }
}
