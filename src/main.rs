use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};
use git2::{Repository, Sort};
use std::io::{self, Write};
use std::process::Command;

fn find_git_commits(limit: usize) -> Vec<String> {
    let repo = Repository::discover(".").expect("failed to open git repository");
    let mut revwalk = repo.revwalk().expect("failed to create revwalk");
    revwalk
        .set_sorting(Sort::TIME)
        .expect("failed to set sorting");
    revwalk.push_head().expect("failed to push HEAD");

    revwalk
        .take(limit)
        .filter_map(|oid| {
            let oid = oid.ok()?;
            let commit = repo.find_commit(oid).ok()?;
            let summary = commit.summary()?.to_string();
            Some(format!("{} {}", &oid.to_string()[..7], summary))
        })
        .collect()
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

fn run_menu(commits: &[String]) -> Option<usize> {
    let mut stdout = io::stdout();
    let mut selected = 0usize;

    terminal::enable_raw_mode().expect("failed to enable raw mode");
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).unwrap();

    let result = loop {
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

        for (i, commit) in commits.iter().enumerate() {
            let eol = if i + 1 < commits.len() { "\r\n" } else { "" };
            if i == selected {
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

        stdout.flush().unwrap();

        match read_menu_event() {
            MenuEvent::Move(delta) => {
                let next = selected as i32 + delta;
                if next >= 0 && next < commits.len() as i32 {
                    selected = next as usize;
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
    let (_, rows) = terminal::size().expect("failed to get terminal size");
    let limit = (rows as usize).saturating_sub(2);
    let commits = find_git_commits(limit);
    if commits.is_empty() {
        eprintln!("No commits found.");
        return;
    }

    if let Some(index) = run_menu(&commits) {
        let sha = &commits[index][..7];
        create_fixup_commit(sha);
    }
}
