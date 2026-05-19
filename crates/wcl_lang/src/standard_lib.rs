use crate::eval::EmbeddedLibrary;

pub const CODECS_LIBRARY_WCL: &str = include_str!("std/codecs.wcl");
pub const HTML_LIBRARY_WCL: &str = include_str!("std/html.wcl");
pub const SVG_LIBRARY_WCL: &str = include_str!("std/svg.wcl");
pub const CSS_LIBRARY_WCL: &str = include_str!("std/css.wcl");
pub const WDOC_LIBRARY_WCL: &str = include_str!("std/wdoc.wcl");

macro_rules! embedded_library {
    ($name:literal, $source:expr) => {
        EmbeddedLibrary {
            name: $name,
            path: $name,
            source: $source,
        }
    };
    ($name:literal, $path:literal, $source:expr) => {
        EmbeddedLibrary {
            name: $name,
            path: $path,
            source: $source,
        }
    };
}

pub fn embedded_libraries() -> Vec<EmbeddedLibrary> {
    vec![
        embedded_library!("codecs.wcl", CODECS_LIBRARY_WCL),
        embedded_library!("html.wcl", HTML_LIBRARY_WCL),
        embedded_library!("svg.wcl", SVG_LIBRARY_WCL),
        embedded_library!("css.wcl", CSS_LIBRARY_WCL),
        embedded_library!("wdoc.wcl", WDOC_LIBRARY_WCL),
        embedded_library!(
            "wdoc/header.wcl",
            "wdoc/header.wcl",
            include_str!("std/wdoc/header.wcl")
        ),
        embedded_library!(
            "wdoc/runtime.wcl",
            "wdoc/runtime.wcl",
            include_str!("std/wdoc/runtime.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/button.wcl",
            "wdoc/widgets/ui/button.wcl",
            include_str!("std/wdoc/widgets/ui/button.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/slider.wcl",
            "wdoc/widgets/ui/slider.wcl",
            include_str!("std/wdoc/widgets/ui/slider.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/phone.wcl",
            "wdoc/widgets/ui/phone.wcl",
            include_str!("std/wdoc/widgets/ui/phone.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/phone_landscape.wcl",
            "wdoc/widgets/ui/phone_landscape.wcl",
            include_str!("std/wdoc/widgets/ui/phone_landscape.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/browser.wcl",
            "wdoc/widgets/ui/browser.wcl",
            include_str!("std/wdoc/widgets/ui/browser.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/window.wcl",
            "wdoc/widgets/ui/window.wcl",
            include_str!("std/wdoc/widgets/ui/window.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/tablet.wcl",
            "wdoc/widgets/ui/tablet.wcl",
            include_str!("std/wdoc/widgets/ui/tablet.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/tablet_landscape.wcl",
            "wdoc/widgets/ui/tablet_landscape.wcl",
            include_str!("std/wdoc/widgets/ui/tablet_landscape.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/input.wcl",
            "wdoc/widgets/ui/input.wcl",
            include_str!("std/wdoc/widgets/ui/input.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/card.wcl",
            "wdoc/widgets/ui/card.wcl",
            include_str!("std/wdoc/widgets/ui/card.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/collapsible_panel.wcl",
            "wdoc/widgets/ui/collapsible_panel.wcl",
            include_str!("std/wdoc/widgets/ui/collapsible_panel.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/avatar.wcl",
            "wdoc/widgets/ui/avatar.wcl",
            include_str!("std/wdoc/widgets/ui/avatar.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/toggle.wcl",
            "wdoc/widgets/ui/toggle.wcl",
            include_str!("std/wdoc/widgets/ui/toggle.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/checkbox.wcl",
            "wdoc/widgets/ui/checkbox.wcl",
            include_str!("std/wdoc/widgets/ui/checkbox.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/radio.wcl",
            "wdoc/widgets/ui/radio.wcl",
            include_str!("std/wdoc/widgets/ui/radio.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/button_group.wcl",
            "wdoc/widgets/ui/button_group.wcl",
            include_str!("std/wdoc/widgets/ui/button_group.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/textbox.wcl",
            "wdoc/widgets/ui/textbox.wcl",
            include_str!("std/wdoc/widgets/ui/textbox.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/dropdown.wcl",
            "wdoc/widgets/ui/dropdown.wcl",
            include_str!("std/wdoc/widgets/ui/dropdown.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/inline_image.wcl",
            "wdoc/widgets/ui/inline_image.wcl",
            include_str!("std/wdoc/widgets/ui/inline_image.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/menubar.wcl",
            "wdoc/widgets/ui/menubar.wcl",
            include_str!("std/wdoc/widgets/ui/menubar.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/context_menu.wcl",
            "wdoc/widgets/ui/context_menu.wcl",
            include_str!("std/wdoc/widgets/ui/context_menu.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/badge.wcl",
            "wdoc/widgets/ui/badge.wcl",
            include_str!("std/wdoc/widgets/ui/badge.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/navbar.wcl",
            "wdoc/widgets/ui/navbar.wcl",
            include_str!("std/wdoc/widgets/ui/navbar.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/stat_card.wcl",
            "wdoc/widgets/ui/stat_card.wcl",
            include_str!("std/wdoc/widgets/ui/stat_card.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/profile_card.wcl",
            "wdoc/widgets/ui/profile_card.wcl",
            include_str!("std/wdoc/widgets/ui/profile_card.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/action_panel.wcl",
            "wdoc/widgets/ui/action_panel.wcl",
            include_str!("std/wdoc/widgets/ui/action_panel.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/list_item.wcl",
            "wdoc/widgets/ui/list_item.wcl",
            include_str!("std/wdoc/widgets/ui/list_item.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/ui/datatable.wcl",
            "wdoc/widgets/ui/datatable.wcl",
            include_str!("std/wdoc/widgets/ui/datatable.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/graph/graph_node.wcl",
            "wdoc/widgets/graph/graph_node.wcl",
            include_str!("std/wdoc/widgets/graph/graph_node.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/chart/charts.wcl",
            "wdoc/widgets/chart/charts.wcl",
            include_str!("std/wdoc/widgets/chart/charts.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/button.wcl",
            "wdoc/widgets/terminal/button.wcl",
            include_str!("std/wdoc/widgets/terminal/button.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/textbox.wcl",
            "wdoc/widgets/terminal/textbox.wcl",
            include_str!("std/wdoc/widgets/terminal/textbox.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/checkbox.wcl",
            "wdoc/widgets/terminal/checkbox.wcl",
            include_str!("std/wdoc/widgets/terminal/checkbox.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/radio.wcl",
            "wdoc/widgets/terminal/radio.wcl",
            include_str!("std/wdoc/widgets/terminal/radio.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/menu.wcl",
            "wdoc/widgets/terminal/menu.wcl",
            include_str!("std/wdoc/widgets/terminal/menu.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/menubar.wcl",
            "wdoc/widgets/terminal/menubar.wcl",
            include_str!("std/wdoc/widgets/terminal/menubar.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/terminal/dropdown.wcl",
            "wdoc/widgets/terminal/dropdown.wcl",
            include_str!("std/wdoc/widgets/terminal/dropdown.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/flowchart/flowchart.wcl",
            "wdoc/widgets/flowchart/flowchart.wcl",
            include_str!("std/wdoc/widgets/flowchart/flowchart.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/flowchart/flow_process.wcl",
            "wdoc/widgets/flowchart/flow_process.wcl",
            include_str!("std/wdoc/widgets/flowchart/flow_process.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/flowchart/flow_decision.wcl",
            "wdoc/widgets/flowchart/flow_decision.wcl",
            include_str!("std/wdoc/widgets/flowchart/flow_decision.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/flowchart/flow_terminal.wcl",
            "wdoc/widgets/flowchart/flow_terminal.wcl",
            include_str!("std/wdoc/widgets/flowchart/flow_terminal.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/flowchart/flow_io.wcl",
            "wdoc/widgets/flowchart/flow_io.wcl",
            include_str!("std/wdoc/widgets/flowchart/flow_io.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/flowchart/flow_subprocess.wcl",
            "wdoc/widgets/flowchart/flow_subprocess.wcl",
            include_str!("std/wdoc/widgets/flowchart/flow_subprocess.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/c4/c4_person.wcl",
            "wdoc/widgets/c4/c4_person.wcl",
            include_str!("std/wdoc/widgets/c4/c4_person.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/c4/c4_system.wcl",
            "wdoc/widgets/c4/c4_system.wcl",
            include_str!("std/wdoc/widgets/c4/c4_system.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/c4/c4_container.wcl",
            "wdoc/widgets/c4/c4_container.wcl",
            include_str!("std/wdoc/widgets/c4/c4_container.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/c4/c4_component.wcl",
            "wdoc/widgets/c4/c4_component.wcl",
            include_str!("std/wdoc/widgets/c4/c4_component.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/c4/c4_boundary.wcl",
            "wdoc/widgets/c4/c4_boundary.wcl",
            include_str!("std/wdoc/widgets/c4/c4_boundary.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/uml/uml_class.wcl",
            "wdoc/widgets/uml/uml_class.wcl",
            include_str!("std/wdoc/widgets/uml/uml_class.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/uml/uml_actor.wcl",
            "wdoc/widgets/uml/uml_actor.wcl",
            include_str!("std/wdoc/widgets/uml/uml_actor.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/uml/uml_package.wcl",
            "wdoc/widgets/uml/uml_package.wcl",
            include_str!("std/wdoc/widgets/uml/uml_package.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/uml/uml_note.wcl",
            "wdoc/widgets/uml/uml_note.wcl",
            include_str!("std/wdoc/widgets/uml/uml_note.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/infra/server.wcl",
            "wdoc/widgets/infra/server.wcl",
            include_str!("std/wdoc/widgets/infra/server.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/infra/database.wcl",
            "wdoc/widgets/infra/database.wcl",
            include_str!("std/wdoc/widgets/infra/database.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/infra/cloud.wcl",
            "wdoc/widgets/infra/cloud.wcl",
            include_str!("std/wdoc/widgets/infra/cloud.wcl")
        ),
        embedded_library!(
            "wdoc/widgets/infra/user.wcl",
            "wdoc/widgets/infra/user.wcl",
            include_str!("std/wdoc/widgets/infra/user.wcl")
        ),
        embedded_library!(
            "wdoc/base_styles.wcl",
            "wdoc/base_styles.wcl",
            include_str!("std/wdoc/base_styles.wcl")
        ),
    ]
}

#[cfg(test)]
mod tests {
    use crate::{parse, ParseOptions};
    use std::path::PathBuf;

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

        let doc = parse(
            super::WDOC_LIBRARY_WCL,
            ParseOptions {
                root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "wdoc.wcl should parse without diagnostics: {:?}",
            doc.errors()
        );
    }

    #[test]
    fn embedded_standard_libraries_resolve_without_filesystem_install() {
        let mut options = ParseOptions {
            no_default_lib_paths: true,
            ..Default::default()
        };
        let doc = parse("import <codecs.wcl>", options.clone());
        assert!(
            !doc.has_errors(),
            "codecs import should resolve from embedded library: {:?}",
            doc.errors()
        );
        assert!(doc
            .imported_paths
            .iter()
            .any(|path| path.ends_with("codecs.wcl")));

        options.lib_paths.push("/definitely/missing".into());
        let doc = parse("import <html.wcl>", options);
        assert!(
            !doc.has_errors(),
            "html import should resolve from embedded library: {:?}",
            doc.errors()
        );
        assert!(doc
            .imported_paths
            .iter()
            .any(|path| path.ends_with("html.wcl")));

        let doc = parse(
            "import <wdoc.wcl>",
            ParseOptions {
                no_default_lib_paths: true,
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "wdoc import should resolve from embedded library: {:?}",
            doc.errors()
        );
        assert!(doc
            .imported_paths
            .iter()
            .any(|path| path.ends_with("wdoc/header.wcl")));
        assert!(doc
            .schemas
            .get_schema("wdoc::draw::graph_node", None)
            .is_some());
        assert!(doc.values.contains_key("wdoc::widget_graph_node"));
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
