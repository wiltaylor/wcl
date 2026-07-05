//! Interactive selection UI for `wcl answer` — arrow-key menus in the spirit
//! of Claude Code's question prompts, with zero terminal dependencies: raw
//! mode is entered by shelling out to `stty` (state saved and restored around
//! each menu; the workspace forbids `unsafe`, so no termios bindings), and
//! everything degrades to plain numbered line input when a real TTY or the
//! `stty` binary isn't available (piped stdin, Windows).
//!
//! Line-input commands follow the REPL's `:quit` convention (`:skip`,
//! `:later`, `:quit`) so they can never collide with a literal answer.

use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::{Command, Stdio};

/// What the respondent chose for one question.
pub(crate) enum Choice {
    /// Picked option indices (0-based into the menu's options) plus optional
    /// free "other" text. Options and text may combine; both empty never
    /// leaves the menu.
    Selection(Vec<usize>, Option<String>),
    /// Mark the question with its declared skipped status.
    Skip,
    /// Leave the question pending and move on (nothing is written).
    Later,
    /// Stop the session; remaining questions stay pending.
    Quit,
}

pub(crate) struct MenuItem {
    pub label: String,
    pub note: Option<String>,
}

/// Present one question. `multi` allows several picks (space to toggle in the
/// arrow-key UI, several numbers in line mode); `skippable` offers the skip
/// affordance. With no `items` this is a free-text question and only the
/// text prompt shows.
pub(crate) fn ask(
    prompt: &str,
    items: &[MenuItem],
    multi: bool,
    skippable: bool,
) -> io::Result<Choice> {
    println!();
    println!("? {prompt}");
    if items.is_empty() {
        return free_text(skippable);
    }
    match RawMode::enter() {
        Some(raw) => arrow_menu(raw, items, multi, skippable),
        None => line_menu(items, multi, skippable),
    }
}

/// Read the free-text answer for an option-less question.
fn free_text(skippable: bool) -> io::Result<Choice> {
    let cmds = if skippable {
        ":skip / :later / :quit"
    } else {
        ":later / :quit"
    };
    loop {
        match line_input(&format!("  type your answer ({cmds})"))? {
            LineRead::Command(c) => {
                if let Some(choice) = command_choice(c, skippable) {
                    return Ok(choice);
                }
            }
            LineRead::Text(t) if t.is_empty() => continue,
            LineRead::Text(t) => return Ok(Choice::Selection(Vec::new(), Some(t))),
        }
    }
}

// ---------------------------------------------------------------------------
// Arrow-key menu (raw mode via `stty`)
// ---------------------------------------------------------------------------

/// Saved `stty -g` state; restores the terminal (and the cursor) on drop.
struct RawMode {
    saved: String,
}

impl RawMode {
    /// Enter raw-ish mode (`-icanon -echo`; `-isig` so Ctrl-C arrives as a
    /// byte we turn into Quit instead of killing the process with the
    /// terminal still raw). `None` when stdin/stdout aren't TTYs or `stty`
    /// is missing/failing — callers fall back to line input.
    fn enter() -> Option<Self> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return None;
        }
        let saved = Command::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let ok = Command::new("stty")
            .args(["-icanon", "-echo", "-isig"])
            .stdin(Stdio::inherit())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return None;
        }
        print!("\x1b[?25l"); // hide cursor while the menu owns the screen
        let _ = io::stdout().flush();
        Some(RawMode { saved })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        print!("\x1b[?25h");
        let _ = io::stdout().flush();
        let _ = Command::new("stty")
            .arg(&self.saved)
            .stdin(Stdio::inherit())
            .status();
    }
}

/// One decoded keypress.
enum Key {
    Up,
    Down,
    Space,
    Enter,
    Char(char),
    Interrupt,
}

fn read_key(stdin: &mut impl Read) -> io::Result<Key> {
    let mut b = [0u8; 1];
    loop {
        stdin.read_exact(&mut b)?;
        match b[0] {
            0x03 | 0x04 => return Ok(Key::Interrupt), // Ctrl-C / Ctrl-D
            b'\r' | b'\n' => return Ok(Key::Enter),
            b' ' => return Ok(Key::Space),
            0x1b => {
                // ESC [ A/B — arrow keys; anything else is ignored.
                let mut seq = [0u8; 2];
                if stdin.read_exact(&mut seq).is_ok() && seq[0] == b'[' {
                    match seq[1] {
                        b'A' => return Ok(Key::Up),
                        b'B' => return Ok(Key::Down),
                        _ => {}
                    }
                }
            }
            c if c.is_ascii_graphic() => return Ok(Key::Char(c as char)),
            _ => {}
        }
    }
}

fn arrow_menu(
    raw: RawMode,
    items: &[MenuItem],
    multi: bool,
    skippable: bool,
) -> io::Result<Choice> {
    // The trailing row is always the free-text escape hatch.
    let other_row = items.len();
    let rows = items.len() + 1;
    let mut cursor = 0usize;
    let mut picked = vec![false; items.len()];
    let mut drawn = 0usize;
    let mut stdin = io::stdin().lock();

    let footer = {
        let mut parts = vec!["↑/↓ move"];
        if multi {
            parts.push("space toggle");
        }
        parts.push("enter accept");
        if skippable {
            parts.push("s skip");
        }
        parts.extend(["l later", "q quit"]);
        parts.join(" · ")
    };

    let choice = loop {
        draw_menu(items, multi, cursor, &picked, &footer, &mut drawn)?;
        match read_key(&mut stdin)? {
            Key::Up => cursor = cursor.checked_sub(1).unwrap_or(rows - 1),
            Key::Down => cursor = (cursor + 1) % rows,
            Key::Char('k') => cursor = cursor.checked_sub(1).unwrap_or(rows - 1),
            Key::Char('j') => cursor = (cursor + 1) % rows,
            Key::Space if multi && cursor < items.len() => picked[cursor] = !picked[cursor],
            Key::Enter => {
                let picks: Vec<usize> = if multi {
                    picked
                        .iter()
                        .enumerate()
                        .filter_map(|(i, &p)| p.then_some(i))
                        .collect()
                } else if cursor < items.len() {
                    vec![cursor]
                } else {
                    Vec::new()
                };
                let wants_other = cursor == other_row;
                if picks.is_empty() && !wants_other {
                    continue; // nothing selected yet — stay in the menu
                }
                // Leave raw mode before reading a text line.
                drop(raw);
                if wants_other {
                    let text = cooked_line("  your answer")?;
                    if text.is_empty() && picks.is_empty() {
                        // Backed out of "other" with nothing picked: re-enter
                        // the menu rather than accept an empty answer.
                        return match RawMode::enter() {
                            Some(raw) => arrow_menu(raw, items, multi, skippable),
                            None => line_menu(items, multi, skippable),
                        };
                    }
                    let text = (!text.is_empty()).then_some(text);
                    return Ok(Choice::Selection(picks, text));
                }
                return Ok(Choice::Selection(picks, None));
            }
            Key::Char('s') if skippable => break Choice::Skip,
            Key::Char('l') => break Choice::Later,
            Key::Char('q') | Key::Interrupt => break Choice::Quit,
            _ => {}
        }
    };
    drop(raw);
    Ok(choice)
}

/// (Re)draw the menu in place: move back over the previously drawn rows,
/// clear each line, print the current state.
fn draw_menu(
    items: &[MenuItem],
    multi: bool,
    cursor: usize,
    picked: &[bool],
    footer: &str,
    drawn: &mut usize,
) -> io::Result<()> {
    let mut out = io::stdout().lock();
    if *drawn > 0 {
        write!(out, "\x1b[{}A", *drawn)?;
    }
    let mut lines = 0usize;
    for (i, item) in items.iter().enumerate() {
        let arrow = if cursor == i { "▸" } else { " " };
        let mark = if multi {
            if picked[i] { "◼ " } else { "◻ " }
        } else {
            ""
        };
        let note = item
            .note
            .as_deref()
            .map(|n| format!("  \x1b[2m{n}\x1b[0m"))
            .unwrap_or_default();
        writeln!(out, "\r\x1b[2K  {arrow} {mark}{}{note}", item.label)?;
        lines += 1;
    }
    let arrow = if cursor == items.len() { "▸" } else { " " };
    writeln!(out, "\r\x1b[2K  {arrow} ✎ other — type your own answer")?;
    writeln!(out, "\r\x1b[2K  \x1b[2m{footer}\x1b[0m")?;
    lines += 2;
    out.flush()?;
    *drawn = lines;
    Ok(())
}

/// Prompt for one line with the terminal back in cooked mode.
fn cooked_line(label: &str) -> io::Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

// ---------------------------------------------------------------------------
// Numbered line-input fallback
// ---------------------------------------------------------------------------

enum LineRead {
    Command(char),
    Text(String),
}

/// Prompt and classify one line: a `:skip` / `:later` / `:quit` command
/// (REPL-style, so it can't collide with a literal answer) or plain text.
/// EOF reads as `:quit`.
fn line_input(hint: &str) -> io::Result<LineRead> {
    println!("\x1b[2m{hint}\x1b[0m");
    print!("> ");
    io::stdout().flush()?;
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line)? == 0 {
        return Ok(LineRead::Command('q'));
    }
    let t = line.trim();
    Ok(match t {
        ":skip" => LineRead::Command('s'),
        ":later" => LineRead::Command('l'),
        ":quit" => LineRead::Command('q'),
        _ => LineRead::Text(t.to_string()),
    })
}

fn command_choice(c: char, skippable: bool) -> Option<Choice> {
    match c {
        's' if skippable => Some(Choice::Skip),
        's' => {
            eprintln!("this question declares no skipped status — use :later or answer it");
            None
        }
        'l' => Some(Choice::Later),
        'q' => Some(Choice::Quit),
        _ => None,
    }
}

fn line_menu(items: &[MenuItem], multi: bool, skippable: bool) -> io::Result<Choice> {
    for (i, item) in items.iter().enumerate() {
        let note = item
            .note
            .as_deref()
            .map(|n| format!("  — {n}"))
            .unwrap_or_default();
        println!("  {}) {}{note}", i + 1, item.label);
    }
    let hint = if multi {
        "  numbers pick options (e.g. \"1 3\"), anything else is your own answer (:skip / :later / :quit)"
    } else {
        "  a number picks an option, anything else is your own answer (:skip / :later / :quit)"
    };
    loop {
        let text = match line_input(hint)? {
            LineRead::Command(c) => match command_choice(c, skippable) {
                Some(choice) => return Ok(choice),
                None => continue,
            },
            LineRead::Text(t) if t.is_empty() => continue,
            LineRead::Text(t) => t,
        };
        if let Some(picks) = parse_picks(&text, items.len()) {
            if !multi && picks.len() > 1 {
                eprintln!("this question takes a single selection");
                continue;
            }
            // Selections can always carry extra free text.
            let extra = match line_input("  anything to add? (empty keeps just the selection)")? {
                LineRead::Text(t) if !t.is_empty() => Some(t),
                _ => None,
            };
            return Ok(Choice::Selection(picks, extra));
        }
        return Ok(Choice::Selection(Vec::new(), Some(text)));
    }
}

/// Interpret input made only of 1-based option numbers (spaces / commas
/// between). Anything else — including out-of-range numbers — is free text.
fn parse_picks(text: &str, len: usize) -> Option<Vec<usize>> {
    let mut picks = Vec::new();
    for tok in text.split([' ', ',']).filter(|t| !t.is_empty()) {
        let n: usize = tok.parse().ok()?;
        if n == 0 || n > len {
            return None;
        }
        let idx = n - 1;
        if !picks.contains(&idx) {
            picks.push(idx);
        }
    }
    (!picks.is_empty()).then_some(picks)
}

#[cfg(test)]
mod tests {
    use super::parse_picks;

    #[test]
    fn numbers_parse_to_zero_based_deduped_picks() {
        assert_eq!(parse_picks("2", 3), Some(vec![1]));
        assert_eq!(parse_picks("1 3", 3), Some(vec![0, 2]));
        assert_eq!(parse_picks("1, 2, 1", 3), Some(vec![0, 1]));
    }

    #[test]
    fn anything_else_is_free_text() {
        assert_eq!(parse_picks("4", 3), None); // out of range
        assert_eq!(parse_picks("0", 3), None); // options are 1-based
        assert_eq!(parse_picks("ship it", 3), None);
        assert_eq!(parse_picks("1 and 2", 3), None);
        assert_eq!(parse_picks("", 3), None);
    }
}
