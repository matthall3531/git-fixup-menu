use git2::{Repository, Sort};

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

fn main() {
    let commits = find_git_commits();
    for commit in commits {
        println!("{}", commit);
    }
}
