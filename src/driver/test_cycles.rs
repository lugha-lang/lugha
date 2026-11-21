//! The cycles a test's run left for the cycle collector: the runtime's
//! `lugha-cycles:` line on the test's stderr, read and taken out by
//! `lughac test`, which prints it as a note under the test's line. It is
//! never a failure.

use super::json::string;

/// How many cycles a test's run freed, how many boxes they held, and
/// where those boxes were made (`file:line:col`).
#[derive(Debug, PartialEq)]
pub(super) struct Cycles {
    pub count: u64,
    pub boxes: u64,
    pub sites: Vec<String>,
}

const PREFIX: &str = "lugha-cycles: ";

/// Takes the runtime's cycles line out of `stderr`, if it has one.
pub(super) fn take(stderr: &mut String) -> Option<Cycles> {
    let line = stderr.lines().find(|l| l.starts_with(PREFIX))?.to_string();
    let rest: String = stderr
        .split_inclusive('\n')
        .filter(|l| !l.starts_with(PREFIX))
        .collect();
    *stderr = rest;
    let mut parts = line[PREFIX.len()..].split('\t');
    let mut numbers = parts.next()?.split(' ');
    let count = numbers.next()?.parse().ok()?;
    let boxes = numbers.next()?.parse().ok()?;
    Some(Cycles {
        count,
        boxes,
        sites: parts.map(str::to_string).collect(),
    })
}

/// The note under a test's line.
pub(super) fn note(cycles: &Cycles) -> String {
    let plural = |n: u64, one: &str, many: &str| match n {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    };
    format!(
        "  note: {} freed ({}), made at {}; a `Weak` back-link avoids them\n",
        plural(cycles.count, "cycle", "cycles"),
        plural(cycles.boxes, "box", "boxes"),
        cycles.sites.join(", ")
    )
}

/// The `--json` field: `,"cycles":{"count":…,"boxes":…,"sites":[…]}`.
pub(super) fn json(cycles: &Cycles) -> String {
    let sites: Vec<String> = cycles.sites.iter().map(|s| string(s)).collect();
    format!(
        ",\"cycles\":{{\"count\":{},\"boxes\":{},\"sites\":[{}]}}",
        cycles.count,
        cycles.boxes,
        sites.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_is_read_and_taken_out() {
        let mut stderr = "warning\nlugha-cycles: 2 4\tt.la:3:9\tt.la:4:9\nmore\n".to_string();
        let cycles = take(&mut stderr).expect("a cycles line");
        assert_eq!(stderr, "warning\nmore\n");
        assert_eq!(
            cycles,
            Cycles {
                count: 2,
                boxes: 4,
                sites: vec!["t.la:3:9".to_string(), "t.la:4:9".to_string()],
            }
        );
        assert_eq!(
            note(&cycles),
            "  note: 2 cycles freed (4 boxes), made at t.la:3:9, t.la:4:9; a `Weak` back-link avoids them\n"
        );
        assert_eq!(
            json(&cycles),
            ",\"cycles\":{\"count\":2,\"boxes\":4,\"sites\":[\"t.la:3:9\",\"t.la:4:9\"]}"
        );
    }

    #[test]
    fn without_the_line_nothing_changes() {
        let mut stderr = "panic: no\n".to_string();
        assert_eq!(take(&mut stderr), None);
        assert_eq!(stderr, "panic: no\n");
    }
}
