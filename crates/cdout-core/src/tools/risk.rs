//! Classifies a proposed command so the UI can skip the approval click for
//! the ones that cannot change anything.
//!
//! **Deny by default.** A command is only [`CommandRisk::ReadOnly`] if every
//! part of it is recognised as read-only; anything unparsed, unrecognised or
//! write-capable is [`CommandRisk::Mutating`]. The failure mode is therefore
//! an unnecessary approval prompt, never an unwanted execution — which is the
//! only direction a classifier like this is allowed to be wrong in.
//!
//! Deliberately *not* clever: no shell grammar, no control flow. A loop or a
//! subshell is treated as mutating even when its body is a probe, because
//! recognising `for f in *; do ffprobe "$f"; done` correctly means
//! implementing enough of zsh to be wrong in interesting ways. Single-command
//! probes are the common case and the whole win.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRisk {
    /// Cannot create, modify or delete anything. Safe to run unattended.
    ReadOnly,
    /// Writes, or cannot be proven not to. Needs approval.
    Mutating,
    /// Recognisably destructive or privilege-escalating. Always needs
    /// approval, even when the user has opted into auto-approving everything.
    Dangerous,
}

/// Substrings that mark a command as [`CommandRisk::Dangerous`]. Matched
/// case-insensitively against the whole command, so they catch a destructive
/// fragment buried in a longer pipeline.
const DANGEROUS: &[&str] = &[
    "rm -rf /",
    "rm -fr /",
    "sudo ",
    "mkfs",
    "diskutil erase",
    "diskutil partition",
    "dd if=",
    "of=/dev/",
    ":(){",
    "shutdown",
    "reboot",
    "format c:",
    "remove-item -recurse -force c:\\",
    "chmod 777 /",
    "chown -r root",
    "> /dev/disk",
    "> /dev/rdisk",
    "killall -9",
    "launchctl unload",
    "csrutil disable",
    "spctl --master-disable",
];

/// Commands that only read. Anything absent from this list is mutating by
/// definition of the policy above.
#[cfg(not(target_os = "windows"))]
const READ_ONLY: &[&str] = &[
    // Filesystem inspection
    "ls",
    "pwd",
    "stat",
    "file",
    "du",
    "df",
    "basename",
    "dirname",
    "readlink",
    "realpath",
    "dirs",
    "test",
    "[",
    "find",
    // Content inspection
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "sort",
    "uniq",
    "cut",
    "column",
    "diff",
    "cmp",
    "md5",
    "shasum",
    "strings",
    "hexdump",
    "xxd",
    "od",
    // Media/metadata probes
    "ffprobe",
    "mediainfo",
    "sips",
    "mdls",
    "identify",
    "iinfo",
    "oiiotool",
    "exiftool",
    "pdfinfo",
    "soxi",
    // Environment
    "echo",
    "printf",
    "which",
    "type",
    "command",
    "env",
    "printenv",
    "date",
    "uname",
    "whoami",
    "hostname",
    "id",
    "locale",
    "sw_vers",
    "arch",
    // Process/system inspection
    "ps",
    "top",
    "uptime",
    "sysctl",
    "vm_stat",
];

#[cfg(target_os = "windows")]
const READ_ONLY: &[&str] = &[
    // Filesystem inspection
    "get-childitem",
    "gci",
    "dir",
    "ls",
    "get-item",
    "gi",
    "get-location",
    "pwd",
    "test-path",
    "resolve-path",
    "get-itemproperty",
    "split-path",
    "join-path",
    "convert-path",
    // Content inspection
    "get-content",
    "gc",
    "cat",
    "type",
    "select-string",
    "measure-object",
    "select-object",
    "where-object",
    "sort-object",
    "group-object",
    "compare-object",
    "get-filehash",
    "format-list",
    "format-table",
    "out-string",
    // Media/metadata probes
    "ffprobe",
    "mediainfo",
    "identify",
    "iinfo",
    "oiiotool",
    "exiftool",
    // Environment
    "write-output",
    "write-host",
    "echo",
    "get-command",
    "get-date",
    "get-host",
    "get-variable",
    "hostname",
    "whoami",
    "get-computerinfo",
    // Process/system inspection
    "get-process",
    "get-service",
    "get-psdrive",
];

/// Guard clauses for commands that read *or* write depending on their flags.
/// Presence of any of these flags disqualifies the command.
const CONDITIONAL: &[(&str, &[&str])] = &[
    // `find` deletes and execs.
    (
        "find",
        &["-delete", "-exec", "-execdir", "-ok", "-fprint", "-fls"],
    ),
    // `test`/`[` are read-only, but `-nt`-style usage is fine; nothing to bar.
    // `exiftool` writes when given any assignment or -overwrite_original.
    ("exiftool", &["=", "-overwrite_original", "-o "]),
    // `oiiotool` writes with -o.
    ("oiiotool", &["-o "]),
    // `sips` writes with these.
    (
        "sips",
        &["--out", "-o ", "--resampleWidth", "--rotate", "--crop"],
    ),
    // `sort`/`grep` can write via these flags.
    ("sort", &["-o "]),
    ("grep", &["--output"]),
    // PowerShell formatters are fine; Get-Content with -OutVariable is fine.
    // `identify` writes nothing, but ImageMagick's `identify -write` does.
    ("identify", &["-write"]),
];

/// Redirections that do not create or modify a file the user cares about.
/// Stripped before the write check so ordinary `2>&1` plumbing stays
/// read-only.
const BENIGN_REDIRECTS: &[&str] = &[
    "2>&1",
    "2>/dev/null",
    "2> /dev/null",
    ">/dev/null",
    "> /dev/null",
    "1>/dev/null",
    "1> /dev/null",
    "2>$null",
    "2> $null",
];

/// Classify `command`. See the module docs for the policy.
pub fn classify(command: &str) -> CommandRisk {
    let lower = command.to_lowercase();

    if DANGEROUS.iter().any(|p| lower.contains(p)) {
        return CommandRisk::Dangerous;
    }

    let trimmed = command.trim();
    if trimmed.is_empty() {
        return CommandRisk::Mutating;
    }

    // Command substitution is allowed only if what it runs is itself
    // read-only; the inner text is then removed so it cannot confuse the
    // segment scan. Backticks are rejected outright — nested quoting makes
    // them not worth parsing.
    if command.contains('`') {
        return CommandRisk::Mutating;
    }
    let (without_subst, subst_ok) = strip_substitutions(command);
    if !subst_ok {
        return CommandRisk::Mutating;
    }

    // Any surviving redirection writes somewhere real.
    let mut scan = without_subst;
    for benign in BENIGN_REDIRECTS {
        scan = scan.replace(benign, " ");
    }
    if scan.contains('>') {
        return CommandRisk::Mutating;
    }

    for segment in scan.split([';', '|', '\n', '&']) {
        if !segment_is_read_only(segment) {
            return CommandRisk::Mutating;
        }
    }

    CommandRisk::ReadOnly
}

/// Remove `$(...)` groups, requiring each to contain a read-only command.
/// Returns the stripped string and whether every group qualified.
fn strip_substitutions(command: &str) -> (String, bool) {
    let mut out = String::with_capacity(command.len());
    let mut rest = command;
    while let Some(start) = rest.find("$(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        // Nested substitution is not worth supporting.
        let Some(end) = after.find(')') else {
            return (out, false);
        };
        let inner = &after[..end];
        if inner.contains("$(") || classify(inner) != CommandRisk::ReadOnly {
            return (out, false);
        }
        // Stand in for the captured value.
        out.push_str("VALUE");
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    (out, true)
}

/// A single `;`/`|`-delimited piece, with assignments and env prefixes peeled
/// off, must name a read-only command.
fn segment_is_read_only(segment: &str) -> bool {
    let mut s = segment.trim();
    if s.is_empty() {
        return true; // trailing separators, `&&` halves, blank lines
    }

    // PowerShell-style `$x = cmd` and POSIX `VAR=value cmd`.
    s = strip_assignments(s);
    if s.is_empty() {
        return true; // pure assignment: `files=(a b)`, `$x = 3`
    }

    let mut tokens = s.split_whitespace();
    let Some(head) = tokens.next() else {
        return true;
    };
    // Reject anything that looks like control flow or a path-invoked binary
    // rather than a bare command name.
    if head.contains('/') || head.contains('\\') {
        return false;
    }
    let name = head.trim_start_matches("./").to_lowercase();
    if !READ_ONLY.contains(&name.as_str()) {
        return false;
    }
    let lower_segment = s.to_lowercase();
    for (cmd, bad_flags) in CONDITIONAL {
        if name == *cmd && bad_flags.iter().any(|f| lower_segment.contains(f)) {
            return false;
        }
    }
    true
}

/// Strip leading assignments so `d=VALUE ffprobe x` and `$d = ffprobe x`
/// are judged on the command they actually run.
fn strip_assignments(segment: &str) -> &str {
    let mut s = segment.trim();
    loop {
        // PowerShell: `$name = rest`
        if let Some(after_sigil) = s.strip_prefix('$') {
            if let Some(eq) = after_sigil.find('=') {
                let name = &after_sigil[..eq];
                if !name.trim().is_empty()
                    && name
                        .trim()
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
                {
                    s = after_sigil[eq + 1..].trim();
                    continue;
                }
            }
        }
        // POSIX: `NAME=value rest`
        let head = s.split_whitespace().next().unwrap_or("");
        if let Some(eq) = head.find('=') {
            let name = &head[..eq];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                s = s[head.len()..].trim_start();
                continue;
            }
        }
        return s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ro(cmd: &str) {
        assert_eq!(
            classify(cmd),
            CommandRisk::ReadOnly,
            "expected read-only: {cmd}"
        );
    }
    fn mutating(cmd: &str) {
        assert_eq!(
            classify(cmd),
            CommandRisk::Mutating,
            "expected mutating: {cmd}"
        );
    }

    #[cfg(not(target_os = "windows"))]
    mod unix {
        use super::*;

        #[test]
        fn plain_probes_are_read_only() {
            ro("ls -la");
            ro("ffprobe -v error -show_entries stream=width,height clip.mov");
            ro("stat -f %z clip.mov");
            ro("file clip.mov");
            ro("wc -l list.txt");
            ro("echo hello");
        }

        #[test]
        fn benign_stderr_plumbing_stays_read_only() {
            // The model writes this constantly; treating it as a write would
            // gut the feature.
            ro("ffprobe -v error clip.mov 2>&1");
            ro("ls -la 2>/dev/null");
        }

        #[test]
        fn read_only_pipelines_are_read_only() {
            ro("ls -la | wc -l");
            ro("cat list.txt | sort | uniq");
            ro("ffprobe -v error clip.mov 2>&1 | head -5");
        }

        #[test]
        fn substitution_of_a_probe_is_read_only() {
            // `d1=$(ffprobe ...)` is how durations get read.
            ro("d1=$(ffprobe -v error -show_entries format=duration clip.mov)");
            ro("echo $(ls)");
        }

        #[test]
        fn substitution_hiding_a_write_is_mutating() {
            mutating("echo $(rm -f clip.mov)");
            mutating("x=$(ffmpeg -i a.mov b.mp4)");
        }

        #[test]
        fn any_real_redirection_is_mutating() {
            mutating("ls > listing.txt");
            mutating("echo hi >> log.txt");
            mutating("ffprobe clip.mov 2>&1 > out.txt");
        }

        #[test]
        fn writers_are_mutating() {
            mutating("ffmpeg -i a.mov b.mp4");
            mutating("rm clip.mov");
            mutating("mv a.mov b.mov");
            mutating("cp a.mov b.mov");
            mutating("mkdir out");
            mutating("touch x");
            mutating("chmod 644 x");
        }

        #[test]
        fn control_flow_is_mutating_even_when_the_body_reads() {
            // Conservative by design — see the module docs.
            mutating("for f in *.mov; do ffprobe \"$f\"; done");
            mutating("while read -r l; do echo $l; done < list.txt");
        }

        #[test]
        fn unknown_commands_are_mutating() {
            mutating("some_new_tool --probe x");
            mutating("./configure");
            mutating("/usr/local/bin/ffprobe x.mov");
        }

        #[test]
        fn backticks_are_mutating() {
            mutating("echo `ls`");
        }

        #[test]
        fn conditional_flags_flip_a_reader_to_mutating() {
            ro("find . -name '*.mov'");
            mutating("find . -name '*.mov' -delete");
            mutating("find . -exec rm {} ;");
            ro("exiftool -CreateDate clip.mov");
            mutating("exiftool -CreateDate=2024:01:01 clip.mov");
            mutating("oiiotool in.exr -o out.exr");
        }

        #[test]
        fn a_read_only_pipeline_with_one_writer_is_mutating() {
            mutating("ls | tee listing.txt");
            mutating("cat a.txt | ffmpeg -i - out.mp4");
        }
    }

    #[cfg(target_os = "windows")]
    mod windows {
        use super::*;

        #[test]
        fn plain_probes_are_read_only() {
            ro("Get-ChildItem -Path .");
            ro("Test-Path output.mp4");
            ro("Get-Content list.txt");
            ro("ffprobe -v error clip.mov");
        }

        #[test]
        fn read_only_pipelines_are_read_only() {
            ro("Get-ChildItem | Measure-Object");
            ro("Get-Content list.txt | Select-Object -First 5");
        }

        #[test]
        fn assignment_of_a_reader_is_read_only() {
            ro("$files = Get-Content 'list.txt'");
        }

        #[test]
        fn writers_are_mutating() {
            mutating("Remove-Item clip.mov");
            mutating("Move-Item a.mov b.mov");
            mutating("New-Item -ItemType Directory out");
            mutating("Set-Content x.txt 'hi'");
            mutating("ffmpeg -i a.mov b.mp4");
        }

        #[test]
        fn redirection_is_mutating() {
            mutating("Get-ChildItem > listing.txt");
        }

        #[test]
        fn control_flow_is_mutating() {
            mutating("foreach ($f in $files) { ffprobe $f }");
        }
    }

    #[test]
    fn destructive_commands_are_dangerous_not_merely_mutating() {
        assert_eq!(classify("sudo rm -rf /"), CommandRisk::Dangerous);
        assert_eq!(
            classify("rm -rf / --no-preserve-root"),
            CommandRisk::Dangerous
        );
        assert_eq!(
            classify("dd if=/dev/zero of=/dev/disk0"),
            CommandRisk::Dangerous
        );
        assert_eq!(classify("mkfs.ext4 /dev/sda1"), CommandRisk::Dangerous);
        assert_eq!(classify("csrutil disable"), CommandRisk::Dangerous);
        assert_eq!(classify("spctl --master-disable"), CommandRisk::Dangerous);
    }

    #[test]
    fn dangerous_wins_over_a_read_only_looking_prefix() {
        // The destructive part is buried after a harmless-looking probe.
        assert_eq!(classify("ls -la; sudo rm -rf /"), CommandRisk::Dangerous);
    }

    #[test]
    fn empty_input_is_never_read_only() {
        mutating("");
        mutating("   ");
    }

    #[test]
    fn case_is_ignored_for_dangerous_matching() {
        assert_eq!(classify("SUDO rm x"), CommandRisk::Dangerous);
    }
}
