//! The Oxide command line tool.
#![deny(missing_docs)]

// Always export the cmd_* modules as public so that it tells us when we are
// missing docs.

mod cmd;
/// The alias command.
pub mod cmd_alias;
/// The completion command.
pub mod cmd_completion;
/// The config command.
pub mod cmd_config;
/// The generate command.
pub mod cmd_generate;
mod colors;
mod config;
mod config_alias;
mod config_file;
mod config_from_env;
mod config_from_file;
mod config_map;
mod context;
mod docs_man;
mod docs_markdown;
mod http;
mod iostreams;
mod update;

use std::io::{Read, Write};

use anyhow::Result;
use clap::Parser;

/// Work seamlessly with Oxide from the command line.
///
/// Environment variables that can be used with oxide.
///
/// OXIDE_TOKEN: an authentication token for Oxide API requests. Setting this
/// avoids being prompted to authenticate and takes precedence over previously
/// stored credentials.
///
/// OXIDE_HOST: specify the Oxide hostname for commands that would otherwise assume
/// the "api.oxide.computer" host.
///
/// OXIDE_BROWSER, BROWSER (in order of precedence): the web browser to use for opening
/// links.
///
/// DEBUG: set to any value to enable verbose output to standard error.
///
/// OXIDE_PAGER, PAGER (in order of precedence): a terminal paging program to send
/// standard output to, e.g. "less".
///
/// NO_COLOR: set to any value to avoid printing ANSI escape sequences for color output.
///
/// CLICOLOR: set to "0" to disable printing ANSI colors in output.
///
/// CLICOLOR_FORCE: set to a value other than "0" to keep ANSI colors in output
/// even when the output is piped.
///
/// OXIDE_FORCE_TTY: set to any value to force terminal-style output even when the
/// output is redirected. When the value is a number, it is interpreted as the number of
/// columns available in the viewport. When the value is a percentage, it will be applied
/// against the number of columns available in the current viewport.
///
/// OXIDE_NO_UPDATE_NOTIFIER: set to any value to disable update notifications. By
/// default, oxide checks for new releases once every 24 hours and displays an upgrade
/// notice on standard error if a newer version was found.
///
/// OXIDE_CONFIG_DIR: the directory where oxide will store configuration files.
/// Default: "$XDG_CONFIG_HOME/oxide" or "$HOME/.config/oxide".
#[derive(Parser, Debug, Clone)]
#[clap(version = clap::crate_version!(), author = clap::crate_authors!("\n"))]
struct Opts {
    /// Print debug info
    #[clap(short, long, global = true, env)]
    debug: bool,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Alias(cmd_alias::CmdAlias),
    Completion(cmd_completion::CmdCompletion),
    Config(cmd_config::CmdConfig),
    Generate(cmd_generate::CmdGenerate),
}

#[tokio::main]
async fn main() -> Result<(), ()> {
    let build_version = clap::crate_version!();
    // Check for updates to the cli.
    // We don't await here since we don't want to block the main thread.
    // We'll check again before we exit.
    let update = crate::update::check_for_update(build_version);

    // Let's get our configuration.
    let mut c = crate::config_file::parse_default_config().unwrap();
    let mut config = crate::config_from_env::EnvConfig::inherit_env(&mut c);
    let mut ctx = crate::context::Context::new(&mut config);

    // Let's grab all our args.
    let args: Vec<String> = std::env::args().collect();
    let result = do_main(args, &mut ctx);

    // If we have an update, let's print it.
    handle_update(&mut ctx, update.await.unwrap_or_default(), build_version).unwrap();

    if let Err(err) = result {
        eprintln!("{}", err);
        std::process::exit(1);
    }

    std::process::exit(result.unwrap_or(0));
}

fn do_main(mut args: Vec<String>, ctx: &mut crate::context::Context) -> Result<i32> {
    let original_args = args.clone();

    // Remove the first argument, which is the program name, and can change depending on how
    // they are calling it.
    args.remove(0);

    let args_str = shlex::join(args.iter().map(|s| s.as_str()).collect::<Vec<&str>>());

    // Check if the user is passing in an alias.
    if !crate::cmd_alias::valid_command(&args_str) {
        // Let's validate if it is an alias.
        // It is okay to check the error here because we will not error out if the
        // alias does not exist. We will just return the expanded args.
        let (mut expanded_args, is_shell) = ctx.config.expand_alias(original_args)?;

        if is_shell {
            // Remove the first argument, since thats our `sh`.
            expanded_args.remove(0);

            let mut external_cmd = std::process::Command::new("sh")
                .args(expanded_args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            let ecode = external_cmd.wait()?;

            // Pipe the output to the terminal.
            if let Some(stdout_rd) = external_cmd.stdout.as_mut() {
                let mut stdout = Vec::new();
                stdout_rd.read_to_end(&mut stdout)?;
                ctx.io.out.write_all(&stdout)?;
            }

            if let Some(mut stderr_rd) = external_cmd.stderr {
                let mut stderr = Vec::new();
                stderr_rd.read_to_end(&mut stderr)?;
                ctx.io.err_out.write_all(&stderr)?;
            }

            return Ok(ecode.code().unwrap_or(0));
        }

        // So we handled if the alias was a shell.
        // We can now parse our options from the extended args.
        args = expanded_args;
    } else {
        args = original_args;
    }

    // Parse the command line arguments.
    let opts: Opts = Opts::parse_from(args);

    // Set our debug flag.
    ctx.debug = opts.debug;

    match opts.subcmd {
        SubCommand::Alias(cmd) => run_cmd(&cmd, ctx),
        SubCommand::Completion(cmd) => run_cmd(&cmd, ctx),
        SubCommand::Config(cmd) => run_cmd(&cmd, ctx),
        SubCommand::Generate(cmd) => run_cmd(&cmd, ctx),
    }

    Ok(0)
}

fn run_cmd(cmd: &impl crate::cmd::Command, ctx: &mut context::Context) {
    if let Err(err) = cmd.run(ctx) {
        writeln!(ctx.io.err_out, "{}", err).unwrap();
        std::process::exit(1);
    }
}

fn handle_update(
    ctx: &mut crate::context::Context,
    update: Option<crate::update::ReleaseInfo>,
    build_version: &str,
) -> Result<()> {
    if let Some(latest_release) = update {
        // do not notify Homebrew users before the version bump had a chance to get merged into homebrew-core
        let is_homebrew = crate::update::is_under_homebrew()?;

        if !(is_homebrew && crate::update::is_recent_release(latest_release.published_at)) {
            let cs = ctx.io.color_scheme();

            writeln!(
                ctx.io.err_out,
                "\n\n{} {} → {}\n",
                cs.yellow("A new release of oxide is available:"),
                cs.cyan(build_version),
                cs.purple(&latest_release.version)
            )?;

            if is_homebrew {
                writeln!(
                    &mut ctx.io.err_out,
                    "To upgrade, run: brew update && brew upgrade oxide"
                )?;
            }

            writeln!(ctx.io.err_out, "{}\n\n", cs.yellow(&latest_release.url))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {

    pub struct TestItem {
        name: String,
        args: Vec<String>,
        want_out: String,
        want_err: String,
        want_code: i32,
    }

    #[test]
    fn test_main() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "existing command".to_string(),
                args: vec!["oxide".to_string(), "completion".to_string()],
                want_out: "complete -F _oxide -o bashdefault -o default oxide\n".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "existing command with args".to_string(),
                args: vec![
                    "oxide".to_string(),
                    "completion".to_string(),
                    "-s".to_string(),
                    "zsh".to_string(),
                ],
                want_out: "_oxide \"$@\"\n".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "add an alias".to_string(),
                args: vec![
                    "oxide".to_string(),
                    "alias".to_string(),
                    "set".to_string(),
                    "foo".to_string(),
                    "completion -s zsh".to_string(),
                ],
                want_out: "- Adding alias for foo: completion -s zsh\n✔ Added alias.".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "add a shell alias".to_string(),
                args: vec![
                    "oxide".to_string(),
                    "alias".to_string(),
                    "set".to_string(),
                    "-s".to_string(),
                    "bar".to_string(),
                    "which bash".to_string(),
                ],
                want_out: "- Adding alias for bar: !which bash\n✔ Added alias.".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "list our aliases".to_string(),
                args: vec!["oxide".to_string(), "alias".to_string(), "list".to_string()],
                want_out: "\"completion -s zsh\"".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "call alias".to_string(),
                args: vec!["oxide".to_string(), "foo".to_string()],
                want_out: "_oxide \"$@\"\n".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "call alias with different binary name".to_string(),
                args: vec!["/bin/thing/oxide".to_string(), "foo".to_string()],
                want_out: "_oxide \"$@\"\n".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
            TestItem {
                name: "call shell alias".to_string(),
                args: vec!["oxide".to_string(), "bar".to_string()],
                want_out: "/bash".to_string(),
                want_err: "".to_string(),
                want_code: 0,
            },
        ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        for t in tests {
            let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            io.set_stdout_tty(false);
            io.set_color_enabled(false);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
            };

            let result = crate::do_main(t.args, &mut ctx);

            let stdout = std::fs::read_to_string(stdout_path).unwrap_or_default();
            let stderr = std::fs::read_to_string(stderr_path).unwrap_or_default();

            assert!(
                stdout.contains(&t.want_out),
                "test {} ->\nstdout: {}\nwant: {}",
                t.name,
                stdout,
                t.want_out
            );

            match result {
                Ok(code) => {
                    assert_eq!(code, t.want_code, "test {}", t.name);
                    assert_eq!(stdout.is_empty(), t.want_out.is_empty(), "test {}", t.name);
                    assert!(stderr.is_empty(), "test {}", t.name);
                }
                Err(err) => {
                    assert!(!t.want_err.is_empty(), "test {}", t.name);
                    assert!(
                        err.to_string().contains(&t.want_err),
                        "test {} -> err: {}\nwant_err: {}",
                        t.name,
                        err,
                        t.want_err
                    );
                    assert_eq!(
                        err.to_string().is_empty(),
                        t.want_err.is_empty(),
                        "test {} -> err: {}\nwant_err: {}",
                        t.name,
                        err,
                        t.want_err
                    );
                    assert!(stderr.is_empty(), "test {}", t.name);
                }
            }
        }
    }
}
