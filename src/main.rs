use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};
use git2::{Repository, Revwalk, Sort};
use std::io::{self, Write};
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

enum MenuEvent {
    Move(i32),
    Confirm,
    Quit,
}

fn read_menu_event() -> MenuEvent {
    loop {
        match event::read().unwrap() {
            Event::Key(key) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => return MenuEvent::Move(-1),
                KeyCode::Down | KeyCode::Char('j') => return MenuEvent::Move(1),
                KeyCode::Enter => return MenuEvent::Confirm,
                KeyCode::Char('q') | KeyCode::Esc => return MenuEvent::Quit,
                _ => {}
            },
            _ => {}
        }
    }
}

fn run_menu(
    commits: &mut Vec<String>,
    repo: &Repository,
    revwalk: &mut Revwalk,
) -> Option<usize> {
    let mut stdout = io::stdout();
    let mut selected = 0usize;
    let mut scroll = 0usize;

    terminal::enable_raw_mode().expect("failed to enable raw mode");
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).unwrap();

    let result = loop {
        let (_, rows) = terminal::size().unwrap();
        let visible_count = (rows as usize).saturating_sub(2);
        let has_more_above = scroll > 0;
        let mut commit_slots = visible_count.saturating_sub(has_more_above as usize);
        let has_more_below = scroll + commit_slots < commits.len();
        if has_more_below {
            commit_slots = commit_slots.saturating_sub(1);
        }
        let visible = &commits[scroll..(scroll + commit_slots).min(commits.len())];

        queue!(
            stdout,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .unwrap();

        queue!(
            stdout,
            style::SetForegroundColor(Color::Yellow),
            style::Print("Select a commit (↑/↓ to move, Enter to confirm, q to quit)\r\n\n"),
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

        for (i, commit) in visible.iter().enumerate() {
            let abs = scroll + i;
            let is_last = i + 1 == visible.len();
            let eol = if !is_last || has_more_below { "\r\n" } else { "" };
            if abs == selected {
                queue!(
                    stdout,
                    style::SetAttribute(Attribute::Reverse),
                    style::Print(format!("> {commit}{eol}")),
                    style::SetAttribute(Attribute::Reset),
                )
                .unwrap();
            } else {
                queue!(stdout, style::Print(format!("  {commit}{eol}"))).unwrap();
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

        match read_menu_event() {
            MenuEvent::Move(delta) => {
                let next = selected as i32 + delta;
                if next >= 0 && next < commits.len() as i32 {
                    selected = next as usize;
                    if selected < scroll {
                        scroll = selected;
                    } else if selected >= scroll + commit_slots {
                        scroll = selected + 1 - commit_slots;
                        // If we just crossed from scroll=0 to scroll>0, has_more_above
                        // becomes true and steals one slot. Compensate by shifting one more.
                        if !has_more_above && scroll > 0 {
                            scroll += 1;
                        }
                    }
                    // Fetch more when the cursor reaches the last screenful
                    if selected + visible_count >= commits.len() {
                        fetch_more(repo, revwalk, visible_count, commits);
                    }
                }
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
    let repo = Repository::discover(".").expect("failed to open git repository");
    let mut revwalk = repo.revwalk().expect("failed to create revwalk");
    revwalk
        .set_sorting(Sort::TIME)
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
