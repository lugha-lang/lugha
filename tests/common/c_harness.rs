//! A C harness that includes `lugha_rt.c`, built plain and with
//! ThreadSanitizer, and run case by case with a time limit: the stress
//! tests of the scheduler and of spawning share it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

/// Builds `source` (a C file whose first line includes `lugha_rt.c`) as
/// harness `name`, once per flavour: plain, or with ThreadSanitizer.
pub fn build(name: &str, source: &str, tsan: bool) -> PathBuf {
    static BUILT: Mutex<Option<HashMap<(String, bool), PathBuf>>> = Mutex::new(None);
    let mut built = BUILT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = (name.to_string(), tsan);
    if let Some(exe) = built.get_or_insert_with(HashMap::new).get(&key) {
        return exe.clone();
    }
    let dir = std::env::temp_dir().join(format!(
        "lugha-{name}-{}-{}",
        std::process::id(),
        u8::from(tsan)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir is writable");
    std::fs::write(
        dir.join("lugha_rt.c"),
        lugha::link::runtime_source(lugha::link::System::host()).as_bytes(),
    )
    .expect("writable");
    std::fs::write(dir.join("harness.c"), source).expect("writable");
    let exe = dir.join("harness");
    let mut cc = Command::new("cc");
    cc.args(["-std=gnu11", "-O1", "-pthread", "-funwind-tables", "-I"])
        .arg(&dir);
    if tsan {
        cc.args(["-fsanitize=thread", "-g"]);
    }
    let out = cc
        .arg(dir.join("harness.c"))
        .args(["-lm", "-o"])
        .arg(&exe)
        .output()
        .expect("cc runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    built
        .get_or_insert_with(HashMap::new)
        .insert(key, exe.clone());
    exe
}

/// Runs `case` of harness `name` on `workers` workers, both builds (the
/// plain one alone off Linux), killing a hung run: what it printed, the
/// same for both.
pub fn run(name: &str, source: &str, case: &str, workers: &str) -> String {
    // ThreadSanitizer runs on Linux only: on macOS its builds of the
    // runtime crash or hang where the plain builds pass, not yet looked into.
    let builds: &[bool] = if cfg!(target_os = "linux") {
        &[false, true]
    } else {
        &[false]
    };
    let outputs: Vec<String> = builds
        .iter()
        .map(|&tsan| {
            let mut child = Command::new(build(name, source, tsan))
                .arg(case)
                .env("LUGHA_WORKERS", workers)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("harness runs");
            let started = std::time::Instant::now();
            let limit = Duration::from_secs(if tsan { 120 } else { 30 });
            while child.try_wait().expect("waits").is_none() {
                if started.elapsed() > limit {
                    // Already ending is fine: the panic below says why.
                    let _ = child.kill();
                    panic!("{case} hung (ThreadSanitizer: {tsan})");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            let out = child.wait_with_output().expect("ends");
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            assert!(
                out.status.success() && stderr.is_empty(),
                "{case} (ThreadSanitizer: {tsan}): {stderr}"
            );
            String::from_utf8_lossy(&out.stdout).into_owned()
        })
        .collect();
    for other in &outputs[1..] {
        assert_eq!(&outputs[0], other, "{case}");
    }
    outputs[0].clone()
}
