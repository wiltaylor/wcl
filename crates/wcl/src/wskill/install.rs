use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::WSKILL_OK;
use super::support::{
    CommandError, discover, open_graph, render_view, report, report_all, scratch,
};

#[derive(Debug)]
struct Generated {
    path: PathBuf,
    producer: PathBuf,
}

#[derive(Debug, Default)]
struct InstallSet {
    skills: BTreeMap<String, Generated>,
    agents: BTreeMap<String, Generated>,
}

/// Run `wcl wskill install [<entry>] [--repo <repo>] [--check]`.
///
/// Rendering always happens in scratch space first. A collection owns the
/// complete generated set; installing one wskill never guesses that another
/// wskill's files are stale.
pub(crate) fn run(entry: &Path, repo: &Path, check: bool) -> u8 {
    let collection = match discover(entry) {
        Ok(collection) => collection,
        Err(error) => return report(error),
    };
    let scratch = match scratch("create install staging directory") {
        Ok(dir) => dir,
        Err(error) => return report(error),
    };
    let generated = match render_install_set(&collection.roots, scratch.path()) {
        Ok(set) => set,
        Err(error) => return report(error),
    };
    let claude = repo.join(".claude");

    if check {
        return check_install(&generated, &claude, collection.complete_set);
    }
    if let Err(error) = write_install(&generated, &claude, collection.complete_set) {
        return report(error);
    }
    println!(
        "installed {} skill{} and {} agent{} into {}",
        generated.skills.len(),
        plural_suffix(generated.skills.len()),
        generated.agents.len(),
        plural_suffix(generated.agents.len()),
        claude.display()
    );
    WSKILL_OK
}

fn render_install_set(roots: &[PathBuf], staging: &Path) -> Result<InstallSet, CommandError> {
    let mut set = InstallSet::default();
    for (root_n, root) in roots.iter().enumerate() {
        let graph = open_graph(root)?;
        let mut entries = HashSet::new();
        for (view_n, view) in graph
            .views
            .iter()
            .filter(|view| view.kind == "ai_skill")
            .enumerate()
        {
            if !entries.insert(view.entry.clone()) {
                continue;
            }
            let out = staging.join(root_n.to_string()).join(view_n.to_string());
            render_view(&graph, view, &out)?;

            let mut skill_files = Vec::new();
            find_files_named(&out, "SKILL.md", &mut skill_files)?;
            for skill_md in skill_files {
                let name = skill_name(&skill_md)?;
                let generated = Generated {
                    path: skill_md.parent().unwrap_or(&out).to_path_buf(),
                    producer: graph.root.clone(),
                };
                if let Some(previous) = set.skills.insert(name.clone(), generated) {
                    return Err(CommandError::Collision(format!(
                        "skill name collision: {name} is produced by {} and {}",
                        previous.producer.display(),
                        graph.root.display()
                    )));
                }
            }

            let agents = out.join("agents");
            if agents.is_dir() {
                for agent in sorted_files(&agents)? {
                    if agent.extension().and_then(|x| x.to_str()) != Some("md") {
                        continue;
                    }
                    let name = agent
                        .file_stem()
                        .and_then(|x| x.to_str())
                        .ok_or_else(|| {
                            CommandError::Invalid(format!(
                                "agent filename is not UTF-8: {}",
                                agent.display()
                            ))
                        })?
                        .to_string();
                    let generated = Generated {
                        path: agent,
                        producer: graph.root.clone(),
                    };
                    if let Some(previous) = set.agents.insert(name.clone(), generated) {
                        return Err(CommandError::Collision(format!(
                            "agent name collision: {name} is declared by {} and {}",
                            previous.producer.display(),
                            graph.root.display()
                        )));
                    }
                }
            }
        }
    }
    Ok(set)
}

fn check_install(set: &InstallSet, claude: &Path, complete_set: bool) -> u8 {
    let mut errors = Vec::new();
    for (name, generated) in &set.skills {
        let installed = claude.join("skills").join(name);
        match dirs_equal(&generated.path, &installed) {
            Ok(true) => {}
            Ok(false) => errors.push(CommandError::Drift(format!(
                "skill artifact drift: {} vs {} — run `wcl wskill install`",
                installed.display(),
                generated.producer.display()
            ))),
            Err(error) => errors.push(error),
        }
    }
    for (name, generated) in &set.agents {
        let installed = claude.join("agents").join(format!("{name}.md"));
        match files_equal(&generated.path, &installed) {
            Ok(true) => {}
            Ok(false) => errors.push(CommandError::Drift(format!(
                "agent artifact drift: {} vs {} — run `wcl wskill install`",
                installed.display(),
                generated.producer.display()
            ))),
            Err(error) => errors.push(error),
        }
    }
    if complete_set {
        match stale_outputs(set, claude) {
            Ok(stale) => {
                errors.extend(stale.skills.into_iter().map(|path| {
                    CommandError::Stale(format!(
                        "stale generated skill: {} — no wskill produces it",
                        path.display()
                    ))
                }));
                errors.extend(stale.agents.into_iter().map(|path| {
                    CommandError::Stale(format!(
                        "stale agent: {} — no wskill produces it",
                        path.display()
                    ))
                }));
            }
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() {
        println!(
            "install check OK — {} skill{}, {} agent{}",
            set.skills.len(),
            plural_suffix(set.skills.len()),
            set.agents.len(),
            plural_suffix(set.agents.len())
        );
        WSKILL_OK
    } else {
        report_all(errors)
    }
}

fn write_install(set: &InstallSet, claude: &Path, complete_set: bool) -> Result<(), CommandError> {
    let skills_dir = claude.join("skills");
    let agents_dir = claude.join("agents");
    fs::create_dir_all(&skills_dir)
        .map_err(|e| CommandError::io(format!("create {}", skills_dir.display()), e))?;
    fs::create_dir_all(&agents_dir)
        .map_err(|e| CommandError::io(format!("create {}", agents_dir.display()), e))?;
    for (name, generated) in &set.skills {
        copy_dir_replacing(&generated.path, &skills_dir.join(name))?;
    }
    for (name, generated) in &set.agents {
        let dest = agents_dir.join(format!("{name}.md"));
        fs::copy(&generated.path, &dest).map_err(|e| {
            CommandError::io(
                format!("copy {} to {}", generated.path.display(), dest.display()),
                e,
            )
        })?;
    }
    if complete_set {
        let stale = stale_outputs(set, claude)?;
        for path in stale.skills {
            fs::remove_dir_all(&path).map_err(|e| {
                CommandError::io(format!("remove stale skill {}", path.display()), e)
            })?;
        }
        for path in stale.agents {
            fs::remove_file(&path).map_err(|e| {
                CommandError::io(format!("remove stale agent {}", path.display()), e)
            })?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct Stale {
    skills: Vec<PathBuf>,
    agents: Vec<PathBuf>,
}

fn stale_outputs(set: &InstallSet, claude: &Path) -> Result<Stale, CommandError> {
    let mut stale = Stale::default();
    let skills_dir = claude.join("skills");
    if skills_dir.is_dir() {
        for dir in sorted_dirs(&skills_dir)? {
            let manifest = dir.join("SKILL.md");
            if !manifest.is_file() {
                continue;
            }
            let source = read_to_string(&manifest)?;
            if !source.contains("wskill_schema_version:") {
                continue;
            }
            let name = utf8_file_name(&dir, "skill directory")?;
            if !set.skills.contains_key(name) {
                stale.skills.push(dir);
            }
        }
    }
    let agents_dir = claude.join("agents");
    if agents_dir.is_dir() {
        for agent in sorted_files(&agents_dir)? {
            if agent.extension().and_then(|x| x.to_str()) != Some("md") {
                continue;
            }
            let source = read_to_string(&agent)?;
            if !source.contains(wcl_wdoc::GENERATED_AGENT_MARKER) {
                continue;
            }
            let name = agent.file_stem().and_then(|x| x.to_str()).ok_or_else(|| {
                CommandError::Invalid(format!("agent filename is not UTF-8: {}", agent.display()))
            })?;
            if !set.agents.contains_key(name) {
                stale.agents.push(agent);
            }
        }
    }
    Ok(stale)
}

fn find_files_named(dir: &Path, name: &str, out: &mut Vec<PathBuf>) -> Result<(), CommandError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).map_err(|e| CommandError::io(format!("read {}", dir.display()), e))?
    {
        let path = entry
            .map_err(|e| CommandError::io(format!("read {}", dir.display()), e))?
            .path();
        if path.is_dir() {
            find_files_named(&path, name, out)?;
        } else if path.file_name().is_some_and(|n| n == name) {
            out.push(path);
        }
    }
    out.sort();
    Ok(())
}

fn skill_name(manifest: &Path) -> Result<String, CommandError> {
    let source = read_to_string(manifest)?;
    let mut lines = source.lines();
    if lines.next() != Some("---") {
        return Err(CommandError::Invalid(format!(
            "generated {} has no YAML frontmatter",
            manifest.display()
        )));
    }
    let raw = lines
        .take_while(|line| *line != "---")
        .find_map(|line| line.strip_prefix("name:"))
        .map(str::trim)
        .ok_or_else(|| {
            CommandError::Invalid(format!(
                "generated {} has no `name` frontmatter",
                manifest.display()
            ))
        })?;
    let name = if raw.starts_with('"') {
        serde_json::from_str::<String>(raw).map_err(|e| {
            CommandError::Invalid(format!(
                "invalid generated skill name in {}: {e}",
                manifest.display()
            ))
        })?
    } else {
        raw.to_string()
    };
    let path = Path::new(&name);
    if name.is_empty()
        || path.components().count() != 1
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
    {
        return Err(CommandError::Invalid(format!(
            "generated {} has unsafe skill name `{name}`",
            manifest.display()
        )));
    }
    Ok(name)
}

fn copy_dir_replacing(source: &Path, dest: &Path) -> Result<(), CommandError> {
    if dest.exists() {
        fs::remove_dir_all(dest)
            .map_err(|e| CommandError::io(format!("replace {}", dest.display()), e))?;
    }
    copy_dir(source, dest)
}

fn copy_dir(source: &Path, dest: &Path) -> Result<(), CommandError> {
    fs::create_dir_all(dest)
        .map_err(|e| CommandError::io(format!("create {}", dest.display()), e))?;
    for path in sorted_paths(source)? {
        let target = dest.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|e| {
                CommandError::io(
                    format!("copy {} to {}", path.display(), target.display()),
                    e,
                )
            })?;
        }
    }
    Ok(())
}

fn dirs_equal(left: &Path, right: &Path) -> Result<bool, CommandError> {
    if !right.is_dir() {
        return Ok(false);
    }
    Ok(dir_snapshot(left)? == dir_snapshot(right)?)
}

fn dir_snapshot(dir: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, CommandError> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<(), CommandError> {
        for path in sorted_paths(dir)? {
            if path.is_dir() {
                walk(root, &path, out)?;
            } else {
                let relative = path.strip_prefix(root).map_err(|e| {
                    CommandError::Invalid(format!("compare {}: {e}", path.display()))
                })?;
                let bytes = fs::read(&path)
                    .map_err(|e| CommandError::io(format!("read {}", path.display()), e))?;
                out.insert(relative.to_path_buf(), bytes);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, CommandError> {
    if !right.is_file() {
        return Ok(false);
    }
    let left =
        fs::read(left).map_err(|e| CommandError::io(format!("read {}", left.display()), e))?;
    let right =
        fs::read(right).map_err(|e| CommandError::io(format!("read {}", right.display()), e))?;
    Ok(left == right)
}

fn sorted_paths(dir: &Path) -> Result<Vec<PathBuf>, CommandError> {
    let mut paths = fs::read_dir(dir)
        .map_err(|e| CommandError::io(format!("read {}", dir.display()), e))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|e| CommandError::io(format!("read {}", dir.display()), e))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, CommandError> {
    Ok(sorted_paths(dir)?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect())
}

fn sorted_files(dir: &Path) -> Result<Vec<PathBuf>, CommandError> {
    Ok(sorted_paths(dir)?
        .into_iter()
        .filter(|path| path.is_file())
        .collect())
}

fn read_to_string(path: &Path) -> Result<String, CommandError> {
    fs::read_to_string(path).map_err(|e| CommandError::io(format!("read {}", path.display()), e))
}

fn utf8_file_name<'a>(path: &'a Path, context: &str) -> Result<&'a str, CommandError> {
    path.file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(|| CommandError::Invalid(format!("{context} is not UTF-8: {}", path.display())))
}

fn plural_suffix(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
