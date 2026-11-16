//! A `Dir` under attack: while a Lugha program reads a file through a
//! folder inside its `Dir` again and again, another thread keeps swapping
//! that folder for a symbolic link to a folder outside. The kernel checks
//! every step of every lookup, so the program never reads the outside file.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Reads `sub/secret.txt` through a `Dir` of `box` 20000 times; prints how
/// many reads saw the inside file, and how many saw anything else.
const PROGRAM: &str = r#"fun main(console: Console, fs: Fs) {
    let box = fs.dir("box").unwrap();
    let (mut inside, mut leaked) = (0, 0);
    for i in 0..20000 {
        match box.read_file("sub/secret.txt") {
            .Ok(text) => if text == "inside" { inside += 1; } else { leaked += 1; },
            .Err(_) => {}
        }
    }
    console.println("{inside} {leaked}");
}
"#;

/// A temporary folder, removed on drop.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn swapping_a_folder_for_a_link_mid_lookup_never_leads_outside() {
    let dir = std::env::temp_dir().join(format!("lugha-dir-race-{}", std::process::id()));
    // A leftover from an earlier run would change what the test sees.
    let _ = std::fs::remove_dir_all(&dir);
    let folder = Folder(dir);
    let root = &folder.0;
    std::fs::create_dir_all(root.join("box/sub")).unwrap();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    std::fs::write(root.join("box/sub/secret.txt"), "inside").unwrap();
    std::fs::write(root.join("outside/secret.txt"), "outside").unwrap();
    std::fs::write(root.join("main.la"), PROGRAM).unwrap();
    let built = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "main.la", "-o", "race"])
        .current_dir(root)
        .env("HOME", root)
        .output()
        .expect("lughac runs");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let done = Arc::new(AtomicBool::new(false));
    let swapper = {
        let (done, root) = (done.clone(), root.clone());
        std::thread::spawn(move || {
            let (sub, real) = (root.join("box/sub"), root.join("box/real"));
            while !done.load(Ordering::Relaxed) {
                // The folder out, a link to outside in its place, then back.
                // Failures just mean the program was mid-lookup: try again.
                let _ = std::fs::rename(&sub, &real);
                let _ = std::os::unix::fs::symlink("../outside", &sub);
                let _ = std::fs::remove_file(&sub);
                let _ = std::fs::rename(&real, &sub);
            }
        })
    };
    let out = Command::new(root.join("race"))
        .current_dir(root)
        .output()
        .expect("the program runs");
    done.store(true, Ordering::Relaxed);
    swapper.join().expect("the swapper ends");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let counts: Vec<u64> = stdout
        .split_whitespace()
        .map(|n| n.parse().expect("two counts"))
        .collect();
    assert_eq!(counts.len(), 2, "{stdout}");
    assert_eq!(counts[1], 0, "read the outside file {} times", counts[1]);
    assert!(counts[0] > 0, "never read the inside file: {stdout}");
}
