use ai_toolbox_core::update;

pub fn run(check_only: bool, json: bool) -> anyhow::Result<()> {
    let status = update::check()?;
    if check_only || !status.available {
        if json {
            println!("{}", serde_json::to_string_pretty(&status)?);
        } else {
            println!(
                "Running ai-toolbox {} · latest stable {}",
                status.current, status.latest
            );
            if status.available {
                println!("Update available: {}", status.release_url);
                if let Some(reason) = &status.installation.blocked {
                    println!("{reason}");
                } else {
                    println!("Run ai-toolbox update to install it.");
                }
            } else {
                println!("No newer release available.");
            }
            if let Some(binary) = &status.installation.binary {
                println!("Binary: {}", binary.display());
            }
            if let Some(app) = &status.installation.app {
                println!("Desktop app: {}", app.display());
            }
            if let Some(catalogue) = &status.installation.catalogue {
                println!("Catalogue: {} (fast-forward only)", catalogue.display());
            }
            for note in &status.installation.notes {
                println!("{note}");
            }
        }
        return Ok(());
    }
    if !json {
        eprintln!(
            "Updating ai-toolbox {} → {}…",
            status.current, status.latest
        );
    }
    let result = update::apply(&status.tag)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print!("{}", result.output);
        println!("Installed ai-toolbox {}. Quit and reopen the desktop app or restart `ai-toolbox ui` to use it.", result.version);
    }
    Ok(())
}
