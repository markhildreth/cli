use anyhow::Result;
use clap::{App, IntoApp, Parser};
use clap_generate::{generate, Shell};

/// Generate shell completion scripts.
///
/// When installing Oxide CLI through a package manager, it's possible that
/// no additional shell configuration is necessary to gain completion support. For
/// Homebrew, see <https://docs.brew.sh/Shell-Completion>.
///
/// If you need to set up completions manually, follow the instructions below. The exact
/// config file locations might vary based on your system. Make sure to restart your
/// shell before testing whether completions are working.
///
/// ### bash
///
/// First, ensure that you install `bash-completion` using your package manager.
///
/// After, add this to your `~/.bash_profile`:
///
///         eval "$(oxide completion -s bash)"
///
/// ### zsh
/// Generate a `_oxide` completion script and put it somewhere in your `$fpath`:
///
///         oxide completion -s zsh > /usr/local/share/zsh/site-functions/_oxide
///
/// Ensure that the following is present in your `~/.zshrc`:
///         autoload -U compinit
///         compinit -i
///
/// Zsh version 5.7 or later is recommended.
///
/// ### fish
///
/// Generate a `oxide.fish` completion script:
///
///         oxide completion -s fish > ~/.config/fish/completions/oxide.fish
///
/// ### PowerShell
///
/// Open your profile script with:
///
///         mkdir -Path (Split-Path -Parent $profile) -ErrorAction SilentlyContinue
///         notepad $profile
///
/// Add the line and save the file:
///
/// Invoke-Expression -Command $(oxide completion -s powershell | Out-String)
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdCompletion {
    /// Shell type: {bash|zsh|fish|powershell}
    #[clap(short, long, default_value = "bash")]
    pub shell: Shell,
}

impl crate::cmd::Command for CmdCompletion {
    fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        // Convert our opts into a clap app.
        let mut app: App = crate::Opts::into_app();
        let name = app.get_name().to_string();
        // Generate the completion script.
        generate(self.shell, &mut app, name, &mut ctx.io.out);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use clap::ArgEnum;

    use crate::cmd::Command;

    use pretty_assertions::assert_eq;

    pub struct TestItem {
        name: String,
        input: String,
        want_out: String,
        want_err: String,
    }

    #[test]
    fn test_cmd_completion_get() {
        let tests = vec![
            TestItem {
                name: "bash completion".to_string(),
                input: "bash".to_string(),
                want_out: "complete -F _oxide -o bashdefault -o default oxide".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "zsh completion".to_string(),
                input: "zsh".to_string(),
                want_out: "#compdef oxide".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "fish completion".to_string(),
                input: "fish".to_string(),
                want_out: "complete -c oxide ".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "PowerShell completion".to_string(),
                input: "powershell".to_string(),
                want_out: "Register-ArgumentCompleter".to_string(),
                want_err: "".to_string(),
            },
            TestItem {
                name: "unsupported shell".to_string(),
                input: "csh".to_string(),
                want_out: "".to_string(),
                want_err: "Invalid variant: csh".to_string(),
            },
        ];

        for t in tests {
            if let Err(e) = clap_generate::Shell::from_str(&t.input, true) {
                assert_eq!(e.to_string(), t.want_err, "test {}", t.name);
                continue;
            }

            let cmd = crate::cmd_completion::CmdCompletion {
                shell: clap_generate::Shell::from_str(&t.input, true).unwrap(),
            };

            let (io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            let mut config = crate::config::new_blank_config().unwrap();
            let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);
            let mut ctx = crate::context::Context { config: &mut c, io };

            cmd.run(&mut ctx).unwrap();

            let stdout = std::fs::read_to_string(&stdout_path).unwrap();
            let stderr = std::fs::read_to_string(&stderr_path).unwrap();

            assert_eq!(stdout.is_empty(), t.want_out.is_empty());
            assert!(stdout.contains(&t.want_out), "test {}", t.name);

            assert_eq!(stderr.is_empty(), t.want_err.is_empty());
            assert!(stderr.contains(&t.want_err), "test {}", t.name);
        }
    }
}
