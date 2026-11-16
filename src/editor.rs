//! Writing text in the user's `$EDITOR`: a private temp file, the editor in
//! the foreground, the text back. The caller owns the terminal handoff.

use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// `$VISUAL`, then `$EDITOR`, then `vi` (empty values are skipped).
pub fn editor_command(env: impl Fn(&str) -> Option<String>) -> String {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .find_map(|key| env(key).filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "vi".to_owned())
}

/// The editor process for `path`. `editor` may carry arguments
/// (`code --wait`), so it runs through `sh`. It never gets the Asana token.
pub fn editor_process(editor: &str, path: &Path) -> Command {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(path)
        .env_remove("ASANA_TOKEN");
    command
}

/// Opens `initial` in `editor` and returns the saved text, or `None` when the
/// user cancels: a non-zero exit (vim's `:cq`) or nothing but whitespace.
/// The temp file is readable by the user only and removed afterwards.
pub fn edit(editor: &str, initial: &str) -> io::Result<Option<String>> {
    let path = temp_path();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(initial.as_bytes())?;
    drop(file);

    let outcome = editor_process(editor, &path).status().and_then(|status| {
        if !status.success() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)?;
        Ok((!text.trim().is_empty()).then_some(text))
    });
    let _ = std::fs::remove_file(&path);
    outcome
}

/// Keeps `text` that couldn't be saved in a private file and returns its
/// path, so nothing the user wrote is lost.
pub fn save_recovery(name: &str, text: &str) -> io::Result<PathBuf> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!("asn-{name}-{nanos}.md"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(text.as_bytes())?;
    Ok(path)
}

/// A new, private temp file path.
fn temp_path() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("asn-edit-{}-{n}-{nanos}.md", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// An executable stub editor running `body` with the file as `$1`.
    fn stub_editor(name: &str, body: &str) -> String {
        let path = std::env::temp_dir().join(format!("asn-stub-{name}-{}", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn prefers_visual_then_editor_then_vi() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(
            editor_command(env(&[("VISUAL", "nvim"), ("EDITOR", "nano")])),
            "nvim"
        );
        assert_eq!(
            editor_command(env(&[("VISUAL", ""), ("EDITOR", "nano")])),
            "nano"
        );
        assert_eq!(editor_command(env(&[])), "vi");
    }

    #[test]
    fn returns_what_the_editor_saved() {
        let editor = stub_editor("write", "printf 'Looks good.\\n' > \"$1\"");

        assert_eq!(edit(&editor, "").unwrap().as_deref(), Some("Looks good.\n"));
    }

    #[test]
    fn the_editor_starts_with_the_initial_text() {
        let editor = stub_editor("append", "printf ' more' >> \"$1\"");

        assert_eq!(
            edit(&editor, "draft").unwrap().as_deref(),
            Some("draft more")
        );
    }

    #[test]
    fn a_failing_editor_exit_cancels() {
        let editor = stub_editor("cq", "printf 'unsaved' > \"$1\"; exit 1");

        assert_eq!(edit(&editor, "").unwrap(), None);
    }

    #[test]
    fn only_whitespace_cancels() {
        let editor = stub_editor("blank", "printf '  \\n\\n' > \"$1\"");

        assert_eq!(edit(&editor, "").unwrap(), None);
    }

    #[test]
    fn editor_arguments_are_kept() {
        let editor = stub_editor("args", "printf '%s' \"$1\" > \"$2\"");

        assert_eq!(
            edit(&format!("{editor} --wait"), "").unwrap().as_deref(),
            Some("--wait")
        );
    }

    #[test]
    fn the_temp_file_is_private_and_removed() {
        let editor = stub_editor(
            "mode",
            "stat -f %Lp \"$1\" > \"$1.mode\" 2>/dev/null || stat -c %a \"$1\" > \"$1.mode\"; printf x > \"$1\"; echo \"$1\" > \"$1.path\"",
        );
        let marker_dir = std::env::temp_dir();

        edit(&editor, "").unwrap();

        let path_file = std::fs::read_dir(&marker_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.to_string_lossy().ends_with(".path") && p.to_string_lossy().contains("asn-edit-")
            })
            .max_by_key(|p| std::fs::metadata(p).unwrap().modified().unwrap())
            .expect("stub recorded the path");
        let edited = std::fs::read_to_string(&path_file)
            .unwrap()
            .trim()
            .to_owned();
        let mode = std::fs::read_to_string(format!("{edited}.mode")).unwrap();
        assert_eq!(mode.trim(), "600");
        assert!(!Path::new(&edited).exists(), "temp file left behind");
        let _ = std::fs::remove_file(format!("{edited}.mode"));
        let _ = std::fs::remove_file(path_file);
    }

    #[test]
    fn the_editor_never_receives_the_asana_token() {
        let command = editor_process("vim", Path::new("/tmp/x"));

        assert!(
            command
                .get_envs()
                .any(|(k, v)| k == "ASANA_TOKEN" && v.is_none())
        );
    }

    #[test]
    fn recovery_files_keep_the_text_privately() {
        let path = save_recovery("description-123", "my long text").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "my long text");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(path.to_string_lossy().contains("asn-description-123"));
        std::fs::remove_file(path).unwrap();
    }
}
