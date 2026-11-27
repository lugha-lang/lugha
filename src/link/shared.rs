//! What a shared library's link adds: `-shared`, and the list of the only
//! symbols it shows, its exports, so the runtime and generated code stay
//! inside it. Each system lists them its own way: a version script (Linux,
//! Android, with the library's own name, its `soname`), an exported-symbols
//! list (macOS), a `.def` file (Windows, with the import library MSVC's
//! linker takes beside the DLL).

use std::ffi::OsString;
use std::io;
use std::path::Path;

use super::System;

/// The link arguments for a shared library `output` showing `symbols`,
/// their list written into `temp`.
pub(super) fn args(
    symbols: &[String],
    (temp, system): (&Path, System),
    output: &Path,
) -> io::Result<Vec<OsString>> {
    let mut args = vec![OsString::from("-shared")];
    match system {
        System::MacOs => {
            let list = temp.join("exports.txt");
            let text: String = symbols.iter().map(|s| format!("_{s}\n")).collect();
            std::fs::write(&list, text)?;
            let mut arg = OsString::from("-Wl,-exported_symbols_list,");
            arg.push(&list);
            args.push(arg);
        }
        System::Windows => {
            let def = temp.join("exports.def");
            let text: String = std::iter::once("EXPORTS\n".to_string())
                .chain(symbols.iter().map(|s| format!("    {s}\n")))
                .collect();
            std::fs::write(&def, text)?;
            args.push(def.into_os_string());
            // The import library MSVC's linker takes, beside the DLL and
            // named after it rather than the build's temporary object.
            let mut implib = OsString::from("-Wl,--out-implib,");
            implib.push(output.with_extension("lib"));
            args.push(implib);
        }
        System::Linux | System::Android => {
            let script = temp.join("exports.map");
            std::fs::write(&script, version_script(symbols))?;
            let mut arg = OsString::from("-Wl,--version-script=");
            arg.push(&script);
            args.push(arg);
            let mut name = OsString::from("-Wl,-soname,");
            name.push(output.file_name().unwrap_or(output.as_os_str()));
            args.push(name);
        }
    }
    Ok(args)
}

/// A version script showing `symbols` and hiding every other.
fn version_script(symbols: &[String]) -> String {
    let global: String = symbols.iter().map(|s| format!("    {s};\n")).collect();
    format!("{{\n  global:\n{global}  local:\n    *;\n}};\n")
}

#[cfg(test)]
mod tests {
    use super::version_script;

    #[test]
    fn a_version_script_shows_only_the_exports() {
        assert_eq!(
            version_script(&["add".into(), "words_longest".into()]),
            "{\n  global:\n    add;\n    words_longest;\n  local:\n    *;\n};\n"
        );
    }
}
