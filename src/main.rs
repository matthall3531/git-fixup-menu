use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};
use git2::{Repository, Revwalk, Sort};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufWriter, Write};
use std::process::Command;

fn fetch_more(repo: &Repository, revwalk: &mut Revwalk, n: usize, commits: &mut Vec<String>) {
    let new: Vec<String> = revwalk
        .by_ref()
        .take(n)
        .filter_map(|oid| {
            let oid = oid.ok()?;
            let commit = repo.find_commit(oid).ok()?;
            let summary = commit.summary()?.to_string();
            Some(format!("{} {}", &oid.to_string()[..7], summary))
        })
        .collect();
    commits.extend(new);
}

// Returns the indices of commits that fit within `limit` display lines starting at `scroll`.
fn collect_visible(
    commits: &[String],
    expanded: &HashSet<usize>,
    bodies: &HashMap<usize, Vec<String>>,
    scroll: usize,
    limit: usize,
) -> Vec<usize> {
    let mut vis = Vec::new();
    let mut lines = 0usize;
    for idx in scroll..commits.len() {
        let h = 1 + if expanded.contains(&idx) {
            bodies.get(&idx).map_or(0, |b| b.len())
        } else {
            0
        };
        if lines + h > limit {
            break;
        }
        vis.push(idx);
        lines += h;
    }
    vis
}

enum MenuEvent {
    Move(i32),
    Expand,
    Collapse,
    Confirm,
    Quit,
}

fn read_menu_event() -> MenuEvent {
    loop {
        match event::read().unwrap() {
            Event::Key(key) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => return MenuEvent::Move(-1),
                KeyCode::Down | KeyCode::Char('j') => return MenuEvent::Move(1),
                KeyCode::Right | KeyCode::Char('l') => return MenuEvent::Expand,
                KeyCode::Left | KeyCode::Char('h') => return MenuEvent::Collapse,
                KeyCode::Enter => return MenuEvent::Confirm,
                KeyCode::Char('q') | KeyCode::Esc => return MenuEvent::Quit,
                _ => {}
            },
            _ => {}
        }
    }
}

fn run_menu(commits: &mut Vec<String>, repo: &Repository, revwalk: &mut Revwalk) -> Option<usize> {
    let mut stdout = BufWriter::new(io::stdout().lock());
    let mut selected = 0usize;
    let mut scroll = 0usize;
    let mut expanded: HashSet<usize> = HashSet::new();
    let mut bodies: HashMap<usize, Vec<String>> = HashMap::new();

    terminal::enable_raw_mode().expect("failed to enable raw mode");
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).unwrap();

    let result = loop {
        let (_, rows) = terminal::size().unwrap();
        let visible_count = (rows as usize).saturating_sub(2);
        let has_more_above = scroll > 0;
        let base_slots = visible_count.saturating_sub(has_more_above as usize);

        // Two-pass: try fitting in base_slots; if we don't reach the end, reserve 1 for indicator.
        let vis_all = collect_visible(commits, &expanded, &bodies, scroll, base_slots);
        let (vis_commits, has_more_below) = if vis_all
            .last()
            .map(|&i| i + 1 < commits.len())
            .unwrap_or(false)
        {
            (
                collect_visible(commits, &expanded, &bodies, scroll, base_slots - 1),
                true,
            )
        } else {
            (vis_all, false)
        };

        // --- Render ---
        queue!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();
        queue!(
            stdout,
            style::SetForegroundColor(Color::Yellow),
            style::Print(
                "Select a commit  ↑/↓ move  →/← expand/collapse  Enter confirm  q quit\r\n\n"
            ),
            style::ResetColor,
        )
        .unwrap();

        if has_more_above {
            queue!(
                stdout,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("  ↑ more commits above...\r\n"),
                style::ResetColor,
            )
            .unwrap();
        }

        for (vi, &abs) in vis_commits.iter().enumerate() {
            let is_last_commit = vi + 1 == vis_commits.len();
            let is_expanded = expanded.contains(&abs);
            let summary_eol = if is_expanded || !is_last_commit || has_more_below {
                "\r\n"
            } else {
                ""
            };

            let sha = &commits[abs][..7];
            let summary = &commits[abs][8..];

            if abs == selected {
                queue!(
                    stdout,
                    style::SetAttribute(Attribute::Reverse),
                    style::Print("> "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(sha),
                    style::ResetColor,
                    style::SetAttribute(Attribute::Reverse),
                    style::Print(format!(" {summary}{summary_eol}")),
                    style::SetAttribute(Attribute::Reset),
                )
                .unwrap();
            } else {
                queue!(
                    stdout,
                    style::Print("  "),
                    style::SetForegroundColor(Color::Green),
                    style::Print(sha),
                    style::ResetColor,
                    style::Print(format!(" {summary}{summary_eol}")),
                )
                .unwrap();
            }

            if is_expanded {
                if let Some(body) = bodies.get(&abs) {
                    for (j, line) in body.iter().enumerate() {
                        let is_last_body = is_last_commit && j + 1 == body.len();
                        let body_eol = if !is_last_body || has_more_below {
                            "\r\n"
                        } else {
                            ""
                        };
                        queue!(
                            stdout,
                            style::SetForegroundColor(Color::Grey),
                            style::Print(format!("    {line}{body_eol}")),
                            style::ResetColor,
                        )
                        .unwrap();
                    }
                }
            }
        }

        if has_more_below {
            queue!(
                stdout,
                style::SetForegroundColor(Color::DarkGrey),
                style::Print("  ↓ more commits below..."),
                style::ResetColor,
            )
            .unwrap();
        }

        stdout.flush().unwrap();

        // --- Events ---
        match read_menu_event() {
            MenuEvent::Move(delta) => {
                let next = selected as i32 + delta;
                if next >= 0 && next < commits.len() as i32 {
                    selected = next as usize;
                    if selected < scroll {
                        scroll = selected;
                    } else if !vis_commits.contains(&selected) {
                        scroll += 1;
                        if !has_more_above && scroll > 0 {
                            scroll += 1;
                        }
                    }
                    if selected + visible_count >= commits.len() {
                        fetch_more(repo, revwalk, visible_count, commits);
                    }
                }
            }
            MenuEvent::Expand => {
                if !bodies.contains_key(&selected) {
                    let sha = &commits[selected][..7];
                    if let Ok(obj) = repo.revparse_single(sha) {
                        if let Ok(commit) = obj.peel_to_commit() {
                            let msg = commit.message().unwrap_or("").to_string();
                            let body: Vec<String> = msg
                                .lines()
                                .skip(1)
                                .skip_while(|l| l.trim().is_empty())
                                .map(|l| l.to_string())
                                .collect();
                            let body = if body.is_empty() {
                                vec!["(no description)".to_string()]
                            } else {
                                body
                            };
                            bodies.insert(selected, body);
                        }
                    }
                }
                expanded.insert(selected);
            }
            MenuEvent::Collapse => {
                expanded.remove(&selected);
            }
            MenuEvent::Confirm => break Some(selected),
            MenuEvent::Quit => break None,
        }
    };

    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show).unwrap();
    terminal::disable_raw_mode().expect("failed to disable raw mode");

    result
}

fn create_fixup_commit(sha: &str) {
    let status = Command::new("git")
        .args(["commit", "--fixup", sha])
        .status()
        .expect("failed to run git commit --fixup");

    if !status.success() {
        eprintln!("git commit --fixup failed");
        std::process::exit(1);
    }
}

fn main() {
    let repo = Repository::discover(".");
    if repo.is_err() {
        eprintln!("Not a valid git repo.");
        return;
    }
    let repo = repo.unwrap();
    let mut revwalk = repo.revwalk().expect("failed to create revwalk");
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .expect("failed to set sorting");
    revwalk.push_head().expect("failed to push HEAD");

    let (_, rows) = terminal::size().expect("failed to get terminal size");
    let initial = (rows as usize) * 2;
    let mut commits = Vec::new();
    fetch_more(&repo, &mut revwalk, initial, &mut commits);

    if commits.is_empty() {
        eprintln!("No commits found.");
        return;
    }

    if let Some(index) = run_menu(&mut commits, &repo, &mut revwalk) {
        let sha = &commits[index][..7];
        create_fixup_commit(sha);
    }
}
