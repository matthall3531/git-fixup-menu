use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{self, Attribute, Color},
    terminal::{self, ClearType},
};
use git2::{Repository, Sort};
use std::io::{self, Write};

fn find_git_commits() -> Vec<String> {
    let repo = Repository::discover(".").expect("failed to open git repository");
    let mut revwalk = repo.revwalk().expect("failed to create revwalk");
    revwalk
        .set_sorting(Sort::TIME)
        .expect("failed to set sorting");
    revwalk.push_head().expect("failed to push HEAD");

    revwalk
        .take(10)
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
        queue!(stdout, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0)).unwrap();

        queue!(
            stdout,
            style::SetForegroundColor(Color::Yellow),
            style::Print("Select a commit (↑/↓ to move, Enter to confirm, q to quit)\r\n\n"),
            style::ResetColor,
        )
        .unwrap();

        for (i, commit) in commits.iter().enumerate() {
            if i == selected {
                queue!(
                    stdout,
                    style::SetAttribute(Attribute::Reverse),
                    style::Print(format!("> {commit}\r\n")),
                    style::SetAttribute(Attribute::Reset),
                )
                .unwrap();
            } else {
                queue!(stdout, style::Print(format!("  {commit}\r\n"))).unwrap();
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

fn main() {
    let commits = find_git_commits();
    if commits.is_empty() {
        eprintln!("No commits found.");
        return;
    }

    if let Some(index) = run_menu(&commits) {
        println!("Selected: {}", commits[index]);
    }
}
