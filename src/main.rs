use std::env;
use std::fs::{create_dir, read_to_string};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration};
use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::Colorize;
use convert_case::{Case, Casing};
use dialoguer::theme::ColorfulTheme;
use include_dir::{include_dir, Dir};
use tera::{Context as Ctx, Tera};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use npdk::debug_println;
use npdk::packer::config_parser::Config;
use npdk::packer::Packer;

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

const TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/template");

impl Cli {
    pub(crate) async fn run(self) -> Result<()> {
        match self.command {
            CliCommand::Pack { source } => {
                pack_plugin(source).await?;
            }
            CliCommand::Init => {
                let dir = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter plugin directory")
                    .default(".".to_string())
                    .interact_text()?;

                let input = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter plugin name")
                    .default("plugin".to_string())
                    .interact_text()?;

                let build_command = dialoguer::Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter build command")
                    .default("npm run build".to_string())
                    .interact_text()?;

                let mut tera = Tera::default();
                tera.add_raw_template(
                    "rsbuild.config.ts",
                    TEMPLATE.get_file("rsbuild.config.ts").unwrap().contents_utf8().unwrap()
                )?;
                tera.add_raw_template(
                    "plugin.conf.toml",
                    TEMPLATE.get_file("plugin.conf.toml").unwrap().contents_utf8().unwrap()
                )?;
                tera.add_raw_template(
                    "package.json",
                    TEMPLATE.get_file("package.json").unwrap().contents_utf8().unwrap()
                )?;
                tera.add_raw_template(
                    "src/index.tsx",
                    TEMPLATE.get_file("src/index.tsx").unwrap().contents_utf8().unwrap()
                )?;
                let mut context = Ctx::new();

                context.insert("pluginName", &input);
                context.insert("pluginNamePascalCase", &input.to_case(Case::Pascal));
                context.insert("buildCommand", &build_command);

                let config_ts = tera.render("rsbuild.config.ts", &context)?;
                let config_toml = tera.render("plugin.conf.toml", &context)?;
                let config_json = tera.render("package.json", &context)?;
                let index = tera.render("src/index.tsx", &context)?;

                create_dir(&dir)?;

                create_template(&PathBuf::from(dir), &TEMPLATE, &RenderedTemplate {config_ts, config_toml, config_json, index}).await?;
            }
            CliCommand::Watch { source } => {
                setup_watcher(source).await?;
                loop {}
            }
        }
        Ok(())
    }
}

async fn pack_plugin(source: Option<String>) -> Result<()> {
    debug_println!("Packing plugin");
    if let Some(source) = source {
        Packer::new(PathBuf::from(source))?.pack().await?;
    } else {
        Packer::new(env::current_dir().unwrap().join("dist"))?.pack().await?;
    }
    Ok(())
}

async fn perform_rebuild(source: Option<String>, build_command: String) -> Result<()> {
    println!("{}", "Rebuilding...".bright_green().bold());
    let build_command_parts: Vec<String> = build_command.split(' ').map(|x| x.to_string()).collect();

    if build_command_parts.is_empty() {
        return Err(anyhow::anyhow!("Build command is empty"));
    }

    let current_dir = env::current_dir()?;

    let output = Command::new("cmd")
        .current_dir(current_dir)
        .args([&["/C".to_string()][..], &build_command_parts].concat())
        .spawn()?
        .wait_with_output()?;

    if output.status.success() {
        println!("{}", "Build succeeded, packing plugin...".bright_green().bold());
        pack_plugin(source).await?;
    } else {
        println!("{}", "Rebuild failed".bright_red().bold());
    }
    Ok(())
}

async fn setup_watcher(source: Option<String>) -> Result<()> {
    let config = toml::from_str::<Config>(&read_to_string(env::current_dir()?.join("plugin.conf.toml"))?)?;
    tokio::spawn(async move {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = notify_debouncer_full::new_debouncer(Duration::from_secs(1), None, tx)?;

        watcher
            .watch(Path::new("."), notify::RecursiveMode::Recursive)?;

        loop {
            if let Ok(Ok(event)) = rx.recv() {
                debug_println!("Received event: {:?}", event);
                if let Some(event) = event.first() {
                    if event.paths.iter().any(|path| {
                        let path_str = path.to_str().unwrap();
                        !path_str.contains("dist") && !path_str.contains("node_modules") && !path_str.contains(".notex.plugin")
                    }) {
                        if !event.kind.is_access() {
                            debug_println!("Triggering rebuild for event: {:?}", event);
                            perform_rebuild(source.clone(), config.profile.build.clone()).await?
                        } else {
                            debug_println!("Event ignored (access): {:?}", event.kind);
                        }
                    } else {
                        debug_println!("Event ignored (path filter): {:?}", event.paths);
                    }
                }
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });
    Ok(())
}
struct RenderedTemplate {
    config_ts: String,
    config_toml: String,
    config_json: String,
    index: String,
}

async fn create_template(main_dir: &PathBuf, dir: &Dir<'_>, t: &RenderedTemplate) -> Result<()> {
    for entry in dir.entries() {
        if entry.as_dir().is_some() {
            create_dir(main_dir.join(entry.path()))?;
            Box::pin(create_template(main_dir, entry.as_dir().unwrap(), &t)).await?
        }
        if entry.as_file().is_some() {
            match entry.path().file_name().unwrap().to_str().unwrap() {
                "rsbuild.config.ts" => File::create(main_dir.join(entry.path())).await?.write_all(t.config_ts.as_bytes()).await?,
                "plugin.conf.toml" => File::create(main_dir.join(entry.path())).await?.write_all(t.config_toml.as_bytes()).await?,
                "package.json" => File::create(main_dir.join(entry.path())).await?.write_all(t.config_json.as_bytes()).await?,
                "index.tsx" => File::create(main_dir.join(entry.path())).await?.write_all(t.index.as_bytes()).await?,
                _ => File::create(main_dir.join(entry.path())).await?.write_all(entry.as_file().unwrap().contents()).await?,
            }
        }
    }
    Ok(())
}

#[derive(Debug, Subcommand, Clone)]
enum CliCommand {
    Pack {
        #[clap(short, long)]
        source: Option<String>,
    },
    Init,
    Watch {
        #[clap(short, long)]
        source: Option<String>,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    cli.run().await?;
    Ok(())
}