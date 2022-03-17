use std::io::Write;

use anyhow::{anyhow, Result};
use clap::Parser;

/// Create, list, edit, view, and delete organizations.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrganization {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser, Debug, Clone)]
enum SubCommand {
    Create(CmdOrganizationCreate),
    Delete(CmdOrganizationDelete),
    Edit(CmdOrganizationEdit),
    List(CmdOrganizationList),
    View(CmdOrganizationView),
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdOrganization {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        match &self.subcmd {
            SubCommand::Create(cmd) => cmd.run(ctx).await,
            SubCommand::Delete(cmd) => cmd.run(ctx).await,
            SubCommand::Edit(cmd) => cmd.run(ctx).await,
            SubCommand::List(cmd) => cmd.run(ctx).await,
            SubCommand::View(cmd) => cmd.run(ctx).await,
        }
    }
}

/// Create a new organization.
///
/// To create a organization interactively, use `oxide organization create` with no arguments.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrganizationCreate {
    /// The name of the organization to create.
    #[clap(name = "organization", default_value = "")]
    pub organization: String,

    /// The description for the organization.
    #[clap(long = "description", short = 'D', default_value = "")]
    pub description: String,
}

// TODO: in interactive create it should default to the user's org.
#[async_trait::async_trait]
impl crate::cmd::Command for CmdOrganizationCreate {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        let mut organization_name = self.organization.to_string();
        let mut description = self.description.to_string();

        if organization_name.is_empty() && !ctx.io.can_prompt() {
            return Err(anyhow!("[organization_name] required in non-interactive mode"));
        }

        if description.is_empty() && !ctx.io.can_prompt() {
            return Err(anyhow!("--description,-D required in non-interactive mode"));
        }

        if organization_name.is_empty() {
            match dialoguer::Input::<String>::new()
                .with_prompt("Organization name:")
                .interact_text()
            {
                Ok(name) => organization_name = name,
                Err(err) => {
                    return Err(anyhow!("prompt failed: {}", err));
                }
            }

            // Propmt for a description if they didn't provide one.
            if self.description.is_empty() {
                match dialoguer::Input::<String>::new()
                    .with_prompt("Organization description:")
                    .interact_text()
                {
                    Ok(desc) => description = desc,
                    Err(err) => {
                        return Err(anyhow!("prompt failed: {}", err));
                    }
                }
            }
        }

        let client = ctx.api_client("")?;

        // Create the organization.
        client
            .organizations()
            .post(&oxide_api::types::RouterCreate {
                name: organization_name.to_string(),
                description: description.to_string(),
            })
            .await?;

        let cs = ctx.io.color_scheme();
        writeln!(
            ctx.io.out,
            "{} Successfully created organization {}",
            cs.success_icon(),
            organization_name
        )?;

        Ok(())
    }
}

/// Delete a organization.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrganizationDelete {
    /// The organization to delete. Can be an ID or name.
    #[clap(name = "organization", required = true)]
    pub organization: String,

    /// Confirm deletion without prompting.
    #[clap(long)]
    pub confirm: bool,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdOrganizationDelete {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if !ctx.io.can_prompt() && !self.confirm {
            return Err(anyhow!("--confirm required when not running interactively"));
        }

        let client = ctx.api_client("")?;

        // Confirm deletion.
        if !self.confirm {
            if let Err(err) = dialoguer::Input::<String>::new()
                .with_prompt(format!("Type {} to confirm deletion:", self.organization))
                .validate_with(|input: &String| -> Result<(), &str> {
                    if input.trim() == self.organization {
                        Ok(())
                    } else {
                        Err("mismatched confirmation")
                    }
                })
                .interact_text()
            {
                return Err(anyhow!("prompt failed: {}", err));
            }
        }

        // Delete the organization.
        client.organizations().delete(&self.organization).await?;

        let cs = ctx.io.color_scheme();
        writeln!(
            ctx.io.out,
            "{} Deleted organization {}",
            cs.success_icon_with_color(ansi_term::Color::Red),
            self.organization
        )?;

        Ok(())
    }
}

/// Edit organization settings.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrganizationEdit {
    /// The organization to edit.
    #[clap(name = "organization", required = true)]
    pub organization: String,

    /// The new name for the organization.
    #[clap(long = "name", short)]
    pub new_name: Option<String>,

    /// The new description for the organization.
    #[clap(long = "description", short = 'D')]
    pub new_description: Option<String>,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdOrganizationEdit {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.new_name.is_none() && self.new_description.is_none() {
            return Err(anyhow!("nothing to edit"));
        }

        let client = ctx.api_client("")?;

        let mut body = oxide_api::types::OrganizationUpdate {
            name: "".to_string(),
            description: "".to_string(),
        };

        let mut name = self.organization.to_string();

        if let Some(n) = &self.new_name {
            body.name = n.to_string();
            // Update the name, so when we print it out in the end, it's correct.
            name = n.to_string();
        }

        if let Some(d) = &self.new_description {
            body.description = d.to_string();
        }

        client.organizations().put(&self.organization, &body).await?;

        let cs = ctx.io.color_scheme();
        if let Some(n) = &self.new_name {
            writeln!(
                ctx.io.out,
                "{} Successfully edited organization {} -> {}",
                cs.success_icon(),
                self.organization,
                n
            )?;
        } else {
            writeln!(
                ctx.io.out,
                "{} Successfully edited organization {}",
                cs.success_icon(),
                name
            )?;
        }

        Ok(())
    }
}

/// List organizations.
///
/// In `--paginate` mode, all pages of results will sequentially be requested until
/// there are no more pages of results.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrganizationList {
    /// Maximum number of organizations to list.
    #[clap(long, short, default_value = "30")]
    pub limit: u32,

    /// Make additional HTTP requests to fetch all pages of organizations.
    #[clap(long)]
    pub paginate: bool,

    /// Output JSON.
    #[clap(long)]
    pub json: bool,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdOrganizationList {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.limit < 1 {
            return Err(anyhow!("--limit must be greater than 0"));
        }

        let client = ctx.api_client("")?;

        let organizations = if self.paginate {
            client
                .organizations()
                .get_all(oxide_api::types::NameSortMode::NameAscending)
                .await?
        } else {
            client
                .organizations()
                .get_page(self.limit, "", oxide_api::types::NameSortMode::NameAscending)
                .await?
        };

        if self.json {
            // If they specified --json, just dump the JSON.
            ctx.io.write_json(&serde_json::json!(organizations))?;
            return Ok(());
        }

        let cs = ctx.io.color_scheme();

        let mut tw = tabwriter::TabWriter::new(vec![]);
        writeln!(tw, "NAME\tDESCRTIPTION\tUPDATED")?;
        for organization in organizations {
            let last_updated = chrono::Utc::now()
                - organization
                    .time_modified
                    .unwrap_or_else(|| organization.time_created.unwrap());
            writeln!(
                tw,
                "{}\t{}\t{}",
                cs.bold(&organization.name),
                &organization.description,
                cs.gray(&chrono_humanize::HumanTime::from(-last_updated).to_string())
            )?;
        }
        tw.flush()?;

        let table = String::from_utf8(tw.into_inner()?)?;
        writeln!(ctx.io.out, "{}", table)?;

        Ok(())
    }
}

/// View a organization.
///
/// Display the description and other information of an Oxide organization.
///
/// With '--web', open the organization in a web browser instead.
#[derive(Parser, Debug, Clone)]
#[clap(verbatim_doc_comment)]
pub struct CmdOrganizationView {
    /// The organization to view.
    #[clap(name = "organization", required = true)]
    pub organization: String,

    /// Open a organization in the browser.
    #[clap(short, long)]
    pub web: bool,

    /// Output JSON.
    #[clap(long)]
    pub json: bool,
}

#[async_trait::async_trait]
impl crate::cmd::Command for CmdOrganizationView {
    async fn run(&self, ctx: &mut crate::context::Context) -> Result<()> {
        if self.web {
            // TODO: make sure this is the correct URL.
            let url = format!(
                "https://{}/{}/{}",
                ctx.config.default_host()?,
                self.organization,
                self.organization
            );

            ctx.browser("", &url)?;
            return Ok(());
        }

        let client = ctx.api_client("")?;

        let organization = client.organizations().get(&self.organization).await?;

        if self.json {
            // If they specified --json, just dump the JSON.
            ctx.io.write_json(&serde_json::json!(organization))?;
            return Ok(());
        }

        let mut tw = tabwriter::TabWriter::new(vec![]);
        writeln!(tw, "id:\t{}", organization.id)?;
        writeln!(tw, "name:\t{}", organization.name)?;
        writeln!(tw, "description:\t{}", organization.description)?;
        if let Some(time_created) = organization.time_created {
            writeln!(
                tw,
                "created:\t{}",
                chrono_humanize::HumanTime::from(chrono::Utc::now() - time_created)
            )?;
        }
        if let Some(time_modified) = organization.time_modified {
            writeln!(
                tw,
                "modified:\t{}",
                chrono_humanize::HumanTime::from(chrono::Utc::now() - time_modified)
            )?;
        }

        tw.flush()?;

        let table = String::from_utf8(tw.into_inner()?)?;
        writeln!(ctx.io.out, "{}", table)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use test_context::{test_context, AsyncTestContext};

    use crate::cmd::Command;

    pub struct TestItem {
        name: String,
        cmd: crate::cmd_org::SubCommand,
        stdin: String,
        want_out: String,
        want_err: String,
    }

    struct TContext {
        orig_oxide_host: Result<String, std::env::VarError>,
        orig_oxide_token: Result<String, std::env::VarError>,
    }

    #[async_trait::async_trait]
    impl AsyncTestContext for TContext {
        async fn setup() -> TContext {
            let orig = TContext {
                orig_oxide_host: std::env::var("OXIDE_HOST"),
                orig_oxide_token: std::env::var("OXIDE_TOKEN"),
            };

            // Set our test values.
            let test_host = std::env::var("OXIDE_TEST_HOST").unwrap_or_default();
            let test_token = std::env::var("OXIDE_TEST_TOKEN").unwrap_or_default();
            std::env::set_var("OXIDE_HOST", test_host);
            std::env::set_var("OXIDE_TOKEN", test_token);

            orig
        }

        async fn teardown(self) {
            // Put the original env var back.
            if let Ok(ref val) = self.orig_oxide_host {
                std::env::set_var("OXIDE_HOST", val);
            } else {
                std::env::remove_var("OXIDE_HOST");
            }

            if let Ok(ref val) = self.orig_oxide_token {
                std::env::set_var("OXIDE_TOKEN", val);
            } else {
                std::env::remove_var("OXIDE_TOKEN");
            }
        }
    }

    #[test_context(TContext)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_cmd_org() {
        let tests: Vec<TestItem> = vec![
            TestItem {
                name: "create no name".to_string(),
                cmd: crate::cmd_org::SubCommand::Create(crate::cmd_org::CmdOrganizationCreate {
                    organization: "".to_string(),
                    description: "".to_string(),
                }),

                stdin: "".to_string(),
                want_out: "".to_string(),
                want_err: "[organization_name] required in non-interactive mode".to_string(),
            },
            TestItem {
                name: "create no description".to_string(),
                cmd: crate::cmd_org::SubCommand::Create(crate::cmd_org::CmdOrganizationCreate {
                    organization: "things".to_string(),
                    description: "".to_string(),
                }),

                stdin: "".to_string(),
                want_out: "".to_string(),
                want_err: "--description,-D required in non-interactive mode".to_string(),
            },
            TestItem {
                name: "delete no --confirm non-interactive".to_string(),
                cmd: crate::cmd_org::SubCommand::Delete(crate::cmd_org::CmdOrganizationDelete {
                    organization: "things".to_string(),
                    confirm: false,
                }),

                stdin: "".to_string(),
                want_out: "".to_string(),
                want_err: "--confirm required when not running interactively".to_string(),
            },
            TestItem {
                name: "list zero limit".to_string(),
                cmd: crate::cmd_org::SubCommand::List(crate::cmd_org::CmdOrganizationList {
                    limit: 0,
                    paginate: false,
                    json: false,
                }),

                stdin: "".to_string(),
                want_out: "".to_string(),
                want_err: "--limit must be greater than 0".to_string(),
            },
            TestItem {
                name: "list --json --paginate".to_string(),
                cmd: crate::cmd_org::SubCommand::List(crate::cmd_org::CmdOrganizationList {
                    limit: 30,
                    paginate: true,
                    json: true,
                }),

                stdin: "".to_string(),
                want_out: "[]\n".to_string(),
                want_err: "".to_string(),
            },
        ];

        let mut config = crate::config::new_blank_config().unwrap();
        let mut c = crate::config_from_env::EnvConfig::inherit_env(&mut config);

        for t in tests {
            let (mut io, stdout_path, stderr_path) = crate::iostreams::IoStreams::test();
            if !t.stdin.is_empty() {
                io.stdin = Box::new(std::io::Cursor::new(t.stdin));
            }
            // We need to also turn off the fancy terminal colors.
            // This ensures it also works in GitHub actions/any CI.
            io.set_color_enabled(false);
            io.set_never_prompt(true);
            let mut ctx = crate::context::Context {
                config: &mut c,
                io,
                debug: false,
            };

            let cmd_org = crate::cmd_org::CmdOrganization { subcmd: t.cmd };
            match cmd_org.run(&mut ctx).await {
                Ok(()) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert!(stderr.is_empty(), "test {}: {}", t.name, stderr);
                    if !stdout.contains(&t.want_out) {
                        assert_eq!(stdout, t.want_out, "test {}: stdout mismatch", t.name);
                    }
                }
                Err(err) => {
                    let stdout = std::fs::read_to_string(stdout_path).unwrap();
                    let stderr = std::fs::read_to_string(stderr_path).unwrap();
                    assert_eq!(stdout, t.want_out, "test {}", t.name);
                    if !err.to_string().contains(&t.want_err) {
                        assert_eq!(err.to_string(), t.want_err, "test {}: err mismatch", t.name);
                    }
                    assert!(stderr.is_empty(), "test {}: {}", t.name, stderr);
                }
            }
        }
    }
}
