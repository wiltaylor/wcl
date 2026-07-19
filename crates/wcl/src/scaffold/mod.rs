//! `wcl init` — scaffold a new project folder from a WCL template.
//!
//! A *template* is a WCL document that `import <scaffold.wcl>`s the
//! embedded scaffold schema (see [`stdlib`]) and declares:
//!   - `property` blocks — questions whose answers the user supplies,
//!   - `file` blocks — generated files and their contents,
//!   - `folder` blocks — directories to create.
//!
//! Generation runs in two passes over the evaluated document. Pass 1
//! reads the `property` declarations (lazy evaluation never forces the
//! file bodies). We then resolve an answer for each property — `-D
//! key=value` on the CLI, an `--answers` file (`.wcl` or `.json`), an
//! interactive prompt, or the property's `default`, in that precedence.
//! Pass 2 re-opens the document with an `answer("name")` builtin bound to
//! the collected answers, so a file's `content` (typically a heredoc with
//! `${answer("name")}`) renders with the user's input substituted, and
//! writes the resulting tree to disk.

mod stdlib;

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use wcl_lang::{
    Block, Document, Environment, EvalError, ParseError, Value, ast, disk_loader, from_fn,
    parse_for_edit,
};

use crate::{EXIT_EVAL, EXIT_IO, EXIT_OK, EXIT_PARSE, EXIT_SCHEMA};

/// Built-in templates shipped with `wcl`, embedded in the binary. The
/// first element of each pair is the name used on the command line
/// (`wcl init <name>`); the second is the template source.
const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    ("minimal", include_str!("templates/minimal.wcl")),
    ("page", include_str!("templates/page.wcl")),
    ("book", include_str!("templates/book.wcl")),
    ("website", include_str!("templates/website.wcl")),
    ("presentation", include_str!("templates/presentation.wcl")),
    ("wskill", include_str!("templates/wskill.wcl")),
    (
        "wskill-registry",
        include_str!("templates/wskill-registry.wcl"),
    ),
    ("wad", include_str!("templates/wad.wcl")),
    ("wplan", include_str!("templates/wplan.wcl")),
];

/// Manifest filename for a folder-based template (a built-in name resolves
/// to embedded source; a user / path template is a folder holding this).
const MANIFEST: &str = "template.wcl";

/// Entry point for the `init` subcommand. Returns a CLI exit code.
pub(crate) fn run_init(
    template: Option<String>,
    dest: Option<PathBuf>,
    answers: Option<PathBuf>,
    defines: Vec<String>,
    defaults: bool,
    force: bool,
    list: bool,
) -> u8 {
    if list {
        list_templates();
        return EXIT_OK;
    }
    let Some(template) = template else {
        eprintln!("error: specify a template name (or `wcl init --list` to see the built-ins)");
        return EXIT_IO;
    };
    match run(
        &template,
        dest,
        answers.as_deref(),
        &defines,
        defaults,
        force,
    ) {
        Ok(()) => EXIT_OK,
        Err(e) => e.report(),
    }
}

fn list_templates() {
    println!("Built-in templates:");
    for (name, _) in BUILTIN_TEMPLATES {
        println!("  {name}");
    }
    if let Some(dir) = templates_dir() {
        let users = user_templates();
        println!("\nUser templates ({}):", dir.display());
        if users.is_empty() {
            println!("  (none — add one as <that dir>/<name>/{MANIFEST})");
        } else {
            for (name, _) in &users {
                println!("  {name}");
            }
        }
    }
    println!(
        "\nUsage: wcl init <template> [dest]   (<template> may also be a path to a .wcl file or a folder containing {MANIFEST})"
    );
}

/// The XDG data directory WCL searches for user templates:
/// `$XDG_DATA_HOME/wcl/templates`, falling back to
/// `$HOME/.local/share/wcl/templates`. `None` when neither var is set.
fn templates_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var_os("HOME").filter(|h| !h.is_empty())?;
            PathBuf::from(home).join(".local").join("share")
        }
    };
    Some(base.join("wcl").join("templates"))
}

/// User templates discovered under [`templates_dir`]: each is a
/// subdirectory holding a `template.wcl` manifest. Returns `(name,
/// manifest_path)` pairs sorted by name.
fn user_templates() -> Vec<(String, PathBuf)> {
    let Some(dir) = templates_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let manifest = path.join(MANIFEST);
        if manifest.is_file()
            && let Some(name) = path.file_name().and_then(|s| s.to_str())
        {
            out.push((name.to_string(), manifest));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A failure during scaffolding, tagged with the matching CLI exit code.
enum InitError {
    Io(String),
    Parse(ParseError),
    Eval(EvalError),
    Schema(Vec<EvalError>),
}

impl InitError {
    fn report(self) -> u8 {
        match self {
            InitError::Io(msg) => {
                eprintln!("error: {msg}");
                EXIT_IO
            }
            InitError::Parse(e) => {
                eprintln!("{:?}", miette::Report::new(e));
                EXIT_PARSE
            }
            InitError::Eval(e) => {
                eprintln!("{:?}", miette::Report::new(e));
                EXIT_EVAL
            }
            InitError::Schema(errs) => {
                let count = errs.len();
                for e in errs {
                    eprintln!("{:?}", miette::Report::new(e));
                }
                eprintln!(
                    "template has {count} schema violation{}",
                    if count == 1 { "" } else { "s" }
                );
                EXIT_SCHEMA
            }
        }
    }
}

/// A property (question) declared by the template.
struct Prop {
    name: String,
    prompt: Option<String>,
    default: Option<String>,
}

/// The files and folders a template generates. Each file is its
/// destination-relative `path` plus the evaluated `content`.
struct Tree {
    files: Vec<(String, String)>,
    folders: Vec<String>,
}

/// The resolved source of a template: its text plus the base directory
/// imports resolve against (`None` for an embedded built-in) and an
/// identifier used to name the destination when none is given.
struct TemplateSrc {
    source: String,
    base_dir: Option<PathBuf>,
    ident: String,
}

fn run(
    template: &str,
    dest: Option<PathBuf>,
    answers_file: Option<&Path>,
    defines: &[String],
    defaults: bool,
    force: bool,
) -> Result<(), InitError> {
    let tpl = resolve_template(template)?;

    // Pass 1 — open with an empty answer map and read the property blocks.
    let discover = open_template(&tpl, empty_env())?;
    let props = read_properties(&discover)?;

    // Resolve answers (CLI > answer file > prompt/default).
    let cli = parse_defines(defines)?;
    let file_answers = match answers_file {
        Some(p) => read_answer_file(p)?,
        None => BTreeMap::new(),
    };
    let interactive = !defaults && crate::atty_stdin();
    let mut answers = BTreeMap::new();
    for p in &props {
        let value = if let Some(v) = cli.get(&p.name) {
            v.clone()
        } else if let Some(v) = file_answers.get(&p.name) {
            v.clone()
        } else if interactive {
            prompt_user(p)?
        } else if let Some(d) = &p.default {
            d.clone()
        } else {
            return Err(InitError::Io(format!(
                "no answer for required property '{name}' (no default; supply `-D {name}=…`, \
                 an `--answers` file, or run interactively)",
                name = p.name
            )));
        };
        answers.insert(p.name.clone(), value);
    }
    let answers = Arc::new(answers);

    // Pass 2 — re-open with the answers bound, validate, read the tree.
    let doc = open_template(&tpl, answer_env(answers.clone()))?;
    let errs = doc.schema_errors();
    if !errs.is_empty() {
        return Err(InitError::Schema(errs));
    }
    let tree = read_tree(&doc)?;

    // Choose and prepare the destination directory.
    let dest = dest.unwrap_or_else(|| {
        answers
            .get("name")
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&tpl.ident))
    });
    prepare_dest(&dest, force)?;

    // Write folders first (so empty ones survive), then files.
    let mut created: Vec<String> = Vec::new();
    for dir in &tree.folders {
        let path = safe_join(&dest, dir)?;
        std::fs::create_dir_all(&path)
            .map_err(|e| InitError::Io(format!("create dir {}: {e}", path.display())))?;
        created.push(format!("{dir}/"));
    }
    for (rel, content) in &tree.files {
        let path = safe_join(&dest, rel)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| InitError::Io(format!("create dir {}: {e}", parent.display())))?;
        }
        std::fs::write(&path, content)
            .map_err(|e| InitError::Io(format!("write {}: {e}", path.display())))?;
        created.push(rel.clone());
    }

    println!("Created {} from template '{}'", dest.display(), tpl.ident);
    for c in &created {
        println!("  {c}");
    }
    Ok(())
}

/// A template's generated output: `(files as (path, content), folders)`.
pub(crate) type TemplateTree = (Vec<(String, String)>, Vec<String>);

/// Evaluate a template (built-in name, user template, or disk path) with
/// the given answers — property defaults fill anything unanswered — and
/// return the generated `(files, folders)` without touching disk. The
/// `wcl editor`'s wskill profile re-add path uses this to scaffold one
/// view's files into an existing wskill.
pub(crate) fn evaluate_template_tree(
    template: &str,
    answers: BTreeMap<String, String>,
) -> Result<TemplateTree, String> {
    fn err(e: InitError) -> String {
        match e {
            InitError::Io(m) => m,
            InitError::Parse(e) => e.to_string(),
            InitError::Eval(e) => e.to_string(),
            InitError::Schema(es) => es
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        }
    }
    let tpl = resolve_template(template).map_err(err)?;
    let discover = open_template(&tpl, empty_env()).map_err(err)?;
    let props = read_properties(&discover).map_err(err)?;
    let mut resolved = BTreeMap::new();
    for p in &props {
        let value = answers
            .get(&p.name)
            .cloned()
            .or_else(|| p.default.clone())
            .ok_or_else(|| format!("no answer for template property `{}`", p.name))?;
        resolved.insert(p.name.clone(), value);
    }
    let doc = open_template(&tpl, answer_env(Arc::new(resolved))).map_err(err)?;
    let tree = read_tree(&doc).map_err(err)?;
    Ok((tree.files, tree.folders))
}

/// Resolve a template argument, in order of precedence: a built-in name
/// (embedded), a user template folder under the XDG data dir, then a path
/// on disk (a `.wcl` file or a folder holding a `template.wcl` manifest).
fn resolve_template(template: &str) -> Result<TemplateSrc, InitError> {
    // 1. A built-in name, embedded in the binary.
    if let Some((name, source)) = BUILTIN_TEMPLATES.iter().find(|(n, _)| *n == template) {
        return Ok(TemplateSrc {
            source: (*source).to_string(),
            base_dir: None,
            ident: (*name).to_string(),
        });
    }
    // 2. A user template: $XDG_DATA_HOME/wcl/templates/<name>/template.wcl.
    if let Some((name, manifest)) = user_templates().into_iter().find(|(n, _)| n == template) {
        return template_from_file(&manifest, name);
    }
    // 3. A path on disk: a `.wcl` file, or a folder holding template.wcl.
    let path = Path::new(template);
    if path.is_file() {
        let ident = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_string();
        return template_from_file(path, ident);
    }
    let dir_manifest = path.join(MANIFEST);
    if dir_manifest.is_file() {
        let ident = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_string();
        return template_from_file(&dir_manifest, ident);
    }

    let mut names: Vec<String> = BUILTIN_TEMPLATES
        .iter()
        .map(|(n, _)| n.to_string())
        .collect();
    names.extend(user_templates().into_iter().map(|(n, _)| n));
    Err(InitError::Io(format!(
        "unknown template '{template}': not a built-in or user template ({}), \
         and not a readable .wcl file or template folder",
        names.join(", ")
    )))
}

/// Build a [`TemplateSrc`] from a manifest `.wcl` file. Imports inside it
/// resolve relative to its directory; `ident` names the destination when
/// none is given on the command line.
fn template_from_file(manifest: &Path, ident: String) -> Result<TemplateSrc, InitError> {
    let source = std::fs::read_to_string(manifest)
        .map_err(|e| InitError::Io(format!("read template {}: {e}", manifest.display())))?;
    Ok(TemplateSrc {
        source,
        base_dir: manifest.parent().map(Path::to_path_buf),
        ident,
    })
}

/// An `Environment` whose `answer("name")` builtin always returns `none`
/// — used in pass 1, where only property declarations are read.
fn empty_env() -> Environment {
    answer_env(Arc::new(BTreeMap::new()))
}

/// An `Environment` exposing `answer(name) -> utf8`, resolving against the
/// collected answers (`none` for an unknown key).
fn answer_env(answers: Arc<BTreeMap<String, String>>) -> Environment {
    let mut env = Environment::new();
    env.add_builtin(
        "answer",
        from_fn(move |name: String| -> Value {
            match answers.get(&name) {
                Some(v) => Value::Utf8(v.clone()),
                None => Value::None,
            }
        }),
    );
    env
}

fn open_template(tpl: &TemplateSrc, env: Environment) -> Result<Document, InitError> {
    let loader = stdlib::schema_registry().loader(disk_loader());
    Document::open_at_with_loader(
        &tpl.source,
        "<template>",
        tpl.base_dir.clone(),
        &env,
        loader,
    )
    .map_err(InitError::Parse)
}

fn read_properties(doc: &Document) -> Result<Vec<Prop>, InitError> {
    let mut props = Vec::new();
    for b in doc.blocks() {
        if b.kind() != "property" {
            continue;
        }
        let name = block_label(&b)?.ok_or_else(|| {
            InitError::Io("a `property` block is missing its name label".to_string())
        })?;
        props.push(Prop {
            name,
            prompt: field_string(&b, "prompt")?,
            default: field_string(&b, "default")?,
        });
    }
    Ok(props)
}

/// Read the `file` and `folder` blocks. Returns `(files, folders)` where
/// each file is `(path, content)`. A block whose `when` field evaluates to
/// `false` is skipped (conditional generation, gated on answers).
fn read_tree(doc: &Document) -> Result<Tree, InitError> {
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for b in doc.blocks() {
        match b.kind() {
            "file" => {
                if field_bool(&b, "when")? == Some(false) {
                    continue;
                }
                let path = block_label(&b)?.ok_or_else(|| {
                    InitError::Io("a `file` block is missing its path label".to_string())
                })?;
                let content = field_string(&b, "content")?.unwrap_or_default();
                files.push((path, content));
            }
            "folder" => {
                if field_bool(&b, "when")? == Some(false) {
                    continue;
                }
                let path = block_label(&b)?.ok_or_else(|| {
                    InitError::Io("a `folder` block is missing its path label".to_string())
                })?;
                folders.push(path);
            }
            _ => {}
        }
    }
    Ok(Tree { files, folders })
}

/// First inline label of `block` as a string, evaluating it.
fn block_label(block: &Block) -> Result<Option<String>, InitError> {
    let labels = block.labels().map_err(InitError::Eval)?;
    Ok(labels.first().and_then(value_string))
}

/// Value of a named field as a string, evaluating it. `None` when the
/// field is absent.
fn field_string(block: &Block, name: &str) -> Result<Option<String>, InitError> {
    match block.field(name) {
        Some(f) => match f.value() {
            Ok(v) => Ok(value_string(v)),
            Err(e) => Err(InitError::Eval(e.clone())),
        },
        None => Ok(None),
    }
}

/// A block's boolean field, evaluated. `None` when absent or not a bool.
fn field_bool(block: &Block, name: &str) -> Result<Option<bool>, InitError> {
    match block.field(name) {
        Some(f) => match f.value() {
            Ok(Value::Bool(b)) => Ok(Some(*b)),
            Ok(_) => Ok(None),
            Err(e) => Err(InitError::Eval(e.clone())),
        },
        None => Ok(None),
    }
}

/// Coerce a scalar `Value` to a string. Strings pass through; `none`
/// yields `None`; other scalars use their display form.
fn value_string(v: &Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        Value::None => None,
        other => Some(other.to_string()),
    }
}

fn parse_defines(defines: &[String]) -> Result<BTreeMap<String, String>, InitError> {
    let mut map = BTreeMap::new();
    for d in defines {
        let (k, v) = d
            .split_once('=')
            .ok_or_else(|| InitError::Io(format!("invalid `-D {d}`: expected key=value")))?;
        map.insert(k.to_string(), v.to_string());
    }
    Ok(map)
}

/// Read an answer file: a `.json` object, or a `.wcl` document whose
/// top-level fields are the answers.
fn read_answer_file(path: &Path) -> Result<BTreeMap<String, String>, InitError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| InitError::Io(format!("read answer file {}: {e}", path.display())))?;
    let is_json = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    let mut map = BTreeMap::new();
    if is_json {
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| InitError::Io(format!("parse {}: {e}", path.display())))?;
        let obj = value.as_object().ok_or_else(|| {
            InitError::Io(format!(
                "{}: answer file must be a JSON object",
                path.display()
            ))
        })?;
        for (k, v) in obj {
            if let Some(s) = json_scalar(v) {
                map.insert(k.clone(), s);
            }
        }
    } else {
        // Evaluate each top-level `key = expr` directly off the AST: a bare
        // answer file has no `@document` schema, so `Document::fields()` /
        // `Field::value()` would fail the strict membership check. A scratch
        // document supplies the evaluation context (literals need nothing).
        let parsed = parse_for_edit(&text, path.display().to_string()).map_err(InitError::Parse)?;
        let scratch = Document::open("", "<answers>").map_err(InitError::Parse)?;
        for item in &parsed.items {
            if let ast::Item::Field(f) = item
                && let Ok(v) = scratch.eval_expr(&f.expr)
                && let Some(s) = value_string(&v)
            {
                map.insert(f.name.clone(), s);
            }
        }
    }
    Ok(map)
}

/// Stringify a JSON scalar; objects/arrays/null are skipped.
fn json_scalar(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn prompt_user(p: &Prop) -> Result<String, InitError> {
    use std::io::Write as _;
    let label = p.prompt.clone().unwrap_or_else(|| p.name.clone());
    match &p.default {
        Some(d) => eprint!("{label} [{d}]: "),
        None => eprint!("{label}: "),
    }
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| InitError::Io(format!("failed to read answer: {e}")))?;
    let trimmed = line.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        Ok(p.default.clone().unwrap_or_default())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Ensure `dest` is safe to write into: created if missing, and either
/// empty or `--force`d.
fn prepare_dest(dest: &Path, force: bool) -> Result<(), InitError> {
    if dest.exists() {
        if !dest.is_dir() {
            return Err(InitError::Io(format!(
                "destination '{}' exists and is not a directory",
                dest.display()
            )));
        }
        let non_empty = std::fs::read_dir(dest)
            .map(|mut it| it.next().is_some())
            .unwrap_or(false);
        if non_empty && !force {
            return Err(InitError::Io(format!(
                "destination '{}' is not empty (use --force to write into it)",
                dest.display()
            )));
        }
    }
    std::fs::create_dir_all(dest)
        .map_err(|e| InitError::Io(format!("create dir {}: {e}", dest.display())))?;
    Ok(())
}

/// Join a template-supplied relative path onto `dest`, rejecting absolute
/// paths and any `..` component so a template can't escape the
/// destination directory.
fn safe_join(dest: &Path, rel: &str) -> Result<PathBuf, InitError> {
    let rp = Path::new(rel);
    for comp in rp.components() {
        match comp {
            Component::ParentDir => {
                return Err(InitError::Io(format!(
                    "template path '{rel}' must not escape the destination (contains `..`)"
                )));
            }
            Component::Prefix(_) | Component::RootDir => {
                return Err(InitError::Io(format!(
                    "template path '{rel}' must be relative"
                )));
            }
            _ => {}
        }
    }
    Ok(dest.join(rp))
}
