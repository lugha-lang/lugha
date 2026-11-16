//! A manifest's inline entry:
//! `{ path = "libs/geo", version = "1.2" }`, the keys in either order, and
//! `{ git = "https://…", version = "1.2" }`, either with the permission
//! list `allow = ["ffi"]`. The same
//! `{ key = "value", … }` form holds a lock file's entries (`modules::lock`).

use super::git;
use super::version::Requirement;

/// Where an entry's package comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A folder, relative to the manifest's.
    Path(String),
    /// A git repository's URL, checked by `git::check_url`.
    Git(String),
}

/// The keys and values of `{ key = "value", … }`, in order, or why the
/// text isn't one. Only `allowed` keys, each at most once.
pub(super) fn pairs(value: &str, allowed: &[&str]) -> Result<Vec<(String, String)>, String> {
    let shape = || format!("expected `{{ {} = \"…\", … }}`", allowed[0]);
    let inner = value
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .ok_or_else(shape)?;
    let mut found: Vec<(String, String)> = Vec::new();
    let mut rest = inner.trim();
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=').ok_or_else(shape)?;
        let key = key.trim();
        let (text, after) = quoted(after.trim_start())
            .ok_or_else(|| format!("the value of `{key}` must be one double-quoted string"))?;
        if !allowed.contains(&key) {
            let keys: Vec<String> = allowed.iter().map(|k| format!("`{k}`")).collect();
            return Err(format!("unknown key `{key}`: only {}", keys.join(", ")));
        }
        if found.iter().any(|(k, _)| k == key) {
            return Err(format!("`{key}` is given twice"));
        }
        found.push((key.to_string(), text));
        rest = after.trim_start();
        // Another key follows a comma; a trailing comma is fine.
        if let Some(next) = rest.strip_prefix(',') {
            rest = next.trim_start();
        } else if !rest.is_empty() {
            return Err(shape());
        }
    }
    Ok(found)
}

/// The value of `key` in `pairs`, if given.
pub(super) fn get(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

/// What an inline entry says: where its package is, the version it
/// requires, and whether it grants the package `ffi`.
pub(super) type Parsed = (Place, Option<Requirement>, bool);

/// `value` without its `allow = [ … ]` pair, and whether that pair
/// grants `ffi`, the one permission; or why the list is wrong.
fn split_allow(value: &str) -> Result<(String, bool), String> {
    let Some(at) = value.find("allow") else {
        return Ok((value.to_string(), false));
    };
    let after = value[at + "allow".len()..].trim_start();
    let list = after
        .strip_prefix('=')
        .map(str::trim_start)
        .filter(|l| l.starts_with('['))
        .ok_or("`allow` takes a list: `allow = [\"ffi\"]`")?;
    let close = list.find(']').ok_or("`allow`'s list has no `]`")?;
    let mut ffi = false;
    for item in list[1..close]
        .split(',')
        .map(str::trim)
        .filter(|i| !i.is_empty())
    {
        match item {
            "\"ffi\"" if !ffi => ffi = true,
            "\"ffi\"" => return Err("`ffi` is allowed twice".into()),
            _ => return Err(format!("`allow` takes only `\"ffi\"`, not {item}")),
        }
    }
    // The rest, with the pair (and a comma beside it) taken out.
    let mut rest = format!("{}{}", &value[..at], &list[close + 1..]);
    rest = rest
        .replace(", ,", ",")
        .replace("{ ,", "{")
        .replace(", }", " }");
    let rest = rest.trim_end().to_string();
    let rest = match rest.strip_suffix(",}") {
        Some(r) => format!("{r}}}"),
        None => rest,
    };
    Ok((rest, ffi))
}

/// The place, the requirement and the `ffi` grant of an inline entry, or
/// why it isn't one.
pub(super) fn parse(value: &str) -> Result<Parsed, String> {
    let (value, ffi) = split_allow(value)?;
    let found = pairs(&value, &["path", "version", "git"])?;
    let version = get(&found, "version");
    let requirement = version.map(|v| Requirement::parse(&v)).transpose()?;
    let place = match (get(&found, "path"), get(&found, "git")) {
        (Some(_), Some(_)) => return Err("an entry has `path` or `git`, not both".into()),
        (Some(path), None) if !path.is_empty() => Place::Path(path),
        (None, Some(url)) => {
            git::check_url(&url)?;
            if requirement.is_none() {
                return Err("a `git` entry needs a `version`, such as `\"1.2\"`".into());
            }
            Place::Git(url)
        }
        _ => return Err("an entry needs a non-empty `path`, or a `git` URL".into()),
    };
    Ok((place, requirement, ffi))
}

/// The string at the start of `text`, between double quotes and without
/// `\`, and what follows it.
fn quoted(text: &str) -> Option<(String, &str)> {
    let body = text.strip_prefix('"')?;
    let end = body.find('"')?;
    let value = &body[..end];
    (!value.contains('\\')).then(|| (value.to_string(), &body[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::{Place, parse};

    #[test]
    fn keys_come_in_either_order() {
        let (place, req, _) = parse("{ path = \"libs/geo\", version = \"1.2\" }").unwrap();
        assert_eq!(place, Place::Path("libs/geo".into()));
        assert_eq!(req.map(|r| r.parts), Some(2));
        let (place, req, _) = parse("{version=\"1\",path=\"a,b\",}").unwrap();
        assert_eq!(
            (place, req.map(|r| r.parts)),
            (Place::Path("a,b".into()), Some(1))
        );
        assert_eq!(
            parse("{ path = \"a\" }").unwrap(),
            (Place::Path("a".into()), None, false)
        );
    }

    #[test]
    fn allow_grants_ffi_anywhere_in_the_entry() {
        for text in [
            "{ path = \"c\", allow = [\"ffi\"] }",
            "{ allow = [\"ffi\"], path = \"c\" }",
            "{ path = \"c\", allow = [ \"ffi\" ], version = \"1\" }",
        ] {
            let (place, _, ffi) = parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!((place, ffi), (Place::Path("c".into()), true), "{text}");
        }
        assert!(!parse("{ path = \"c\", allow = [] }").unwrap().2);
        for bad in [
            "{ path = \"c\", allow = [\"net\"] }",
            "{ path = \"c\", allow = \"ffi\" }",
            "{ path = \"c\", allow = [\"ffi\", \"ffi\"] }",
            "{ path = \"c\", allow = [\"ffi\" }",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn git_entries_need_a_version_and_a_safe_url() {
        let (place, req, _) =
            parse("{ git = \"https://x.org/geo.git\", version = \"1.4\" }").unwrap();
        assert_eq!(place, Place::Git("https://x.org/geo.git".into()));
        assert_eq!(req.map(|r| r.parts), Some(2));
        assert!(parse("{ git = \"file:///srv/geo\", version = \"1\" }").is_ok());
        for bad in [
            "{ git = \"https://x.org/geo.git\" }",
            "{ git = \"https://x.org/g\", path = \"a\", version = \"1\" }",
            "{ git = \"ssh://x.org/g\", version = \"1\" }",
            "{ git = \"git@x.org:g\", version = \"1\" }",
            "{ git = \"ext::sh -c x\", version = \"1\" }",
            "{ git = \"https://me:token@x.org/g\", version = \"1\" }",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            "{ version = \"1.2\" }",
            "{ path = \"a\", url = \"b\" }",
            "{ path = \"a\", path = \"b\" }",
            "{ path = \"a\", version = \"^1\" }",
            "{ path = \"a\"",
            "{ path = a }",
            "{ path = \"a\" version = \"1\" }",
            "{ path = \"\" }",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
