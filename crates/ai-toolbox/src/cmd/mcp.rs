use crate::cli::McpOptimise;
use ai_toolbox_core::mcp::{self, Backend, Context, Options, Scope};
use std::path::Path;

fn context(repo: &Path, backend: Option<&str>) -> anyhow::Result<Context> {
    match backend {
        Some(backend) => Ok(
            Context::storage().with_backend(Some(if backend == "adapter" {
                Backend::Adapter
            } else {
                Backend::Native
            })),
        ),
        None => Ok(Context::current_for(repo)?),
    }
}
pub fn report(repo: &Path, backend: Option<&str>, json: bool) -> anyhow::Result<()> {
    let report = mcp::inspect(&context(repo, backend)?, repo)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "Pi {:?} file configuration: {} known configured eager tools, {} unknown catalogue(s)",
            report.backend, report.known_direct_tools, report.unknown_catalogues
        );
        println!(
            "Counts are before Pi project trust and session overrides, not live availability."
        );
        if !report.complete {
            println!("Partial inventory: these counts are not a complete total.");
        }
        for server in report.servers {
            println!(
                "  {}: {} · direct {} · {}",
                server.name,
                server.exposure,
                server
                    .direct_tools
                    .map_or("unknown".into(), |n| n.to_string()),
                server.exposure_source.display()
            );
            println!("    {}", server.cache_note);
        }
        for warning in report.warnings {
            println!("  Warning: {warning}");
        }
        println!("No servers started. Preview changes with: ai-toolbox mcp-optimise preview");
    }
    Ok(())
}
pub fn run(repo: &Path, action: &McpOptimise, json: bool, dry_run: bool) -> anyhow::Result<()> {
    match action {
        McpOptimise::Preview {
            scope,
            keep_direct,
            keep_tool,
        } => {
            let scope = match scope.as_str() {
                "worktrees" => Scope::Worktrees,
                "all" => Scope::All,
                "machine" => Scope::Machine,
                _ => Scope::Project,
            };
            let mut options = Options {
                scope,
                keep_direct: keep_direct.clone(),
                ..Options::default()
            };
            for choice in keep_tool {
                let (server, tool) = choice
                    .split_once(':')
                    .ok_or_else(|| anyhow::anyhow!("--keep-tool must be SERVER:TOOL"))?;
                options
                    .keep_tools
                    .entry(server.into())
                    .or_default()
                    .push(tool.into());
            }
            let registered = if matches!(scope, Scope::All | Scope::Machine) {
                if !ai_toolbox_core::registry::db::Db::exists() {
                    anyhow::bail!(
                        "Scan/register projects first: ai-toolbox projects scan --root PATH"
                    );
                }
                ai_toolbox_core::registry::Registry::open()?
                    .all()?
                    .into_iter()
                    .filter(|r| r.exists())
                    .map(|r| r.path)
                    .collect()
            } else {
                vec![]
            };
            let prepare = if dry_run {
                mcp::preview_read_only
            } else {
                mcp::preview
            };
            let preview = prepare(&context(repo, None)?, repo, &registered, options)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&preview)?);
            } else {
                println!(
                    "Preview {} — {} exposure change(s) across {} scope(s)",
                    preview.id,
                    preview.changes.len(),
                    preview.reports.len()
                );
                for change in &preview.changes {
                    println!(
                        "  {} · {}: {} → {}",
                        change.path.display(),
                        change.server,
                        change.before,
                        change.after
                    );
                }
                for warning in &preview.warnings {
                    println!("  {warning}");
                }
                if preview.retained {
                    println!(
                        "Apply this exact preview: ai-toolbox mcp-optimise apply {}",
                        preview.id
                    );
                } else {
                    println!("--dry-run: no retained preview or configuration changes. Repeat without --dry-run to retain an applicable preview.");
                }
            }
        }
        McpOptimise::Apply { id } | McpOptimise::Rollback { id } => {
            anyhow::ensure!(!dry_run,"Use mcp-optimise preview to inspect changes; --dry-run never applies or rolls back.");
            let context = Context::storage();
            let result = if matches!(action, McpOptimise::Apply { .. }) {
                mcp::apply(&context, id)?
            } else {
                mcp::rollback(&context, id)?
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Updated {} file(s). Private backup: {}\nRollback: ai-toolbox mcp-optimise rollback {}\nReload/restart Pi; project reapproval may be required.",result.files,result.backup.display(),result.id);
            }
        }
    }
    Ok(())
}
