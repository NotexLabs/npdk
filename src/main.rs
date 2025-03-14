use std::fs::create_dir;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration};
use clap::{Parser, Subcommand};
use anyhow::Result;
use async_watcher::AsyncDebouncer;
use async_watcher::notify::RecursiveMode;
use convert_case::{Case, Casing};
use dialoguer::theme::ColorfulTheme;
use include_dir::{include_dir, Dir};
use notify::{recommended_watcher, Event, EventKind, Watcher};
use notify::event::{CreateKind, ModifyKind};
use tera::{Context, Tera};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::{join, task, time};
use tokio::sync::mpsc;
use tokio::time::Instant;
use walkdir::WalkDir;
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
                if let Some(source) = source {
                    Packer::new(PathBuf::from(source))?.pack().await?;
                } else {
                    Packer::new(std::env::current_dir().unwrap().join("dist"))?.pack().await?
                }

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
                let mut context = Context::new();

                context.insert("pluginName", &input);
                context.insert("pluginNamePascalCase", &input.to_case(Case::Pascal));

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

async fn perform_rebuild(source: Option<String>) -> Result<()> {
    let build_success = task::spawn_blocking(|| {
        Ok::<bool, anyhow::Error> (
            Command::new("bun")
                .args(["run", "build"])
                .spawn()?
                .wait_with_output()?
                .status
                .success()
        )
    })
        .await??;
    if build_success {
        if let Some(source) = source {
            Packer::new(PathBuf::from(source))?.pack().await?;
        } else {
            Packer::new(std::env::current_dir().unwrap().join("dist"))?.pack().await?
        }
    } else {
        println!("Rebuild failed");
    }
    Ok(())
}


async fn setup_watcher(source: Option<String>) -> anyhow::Result<()> {
    tokio::spawn(async move {
        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher = notify_debouncer_full::new_debouncer(Duration::from_secs(1), None, tx)
            .expect("failed to start builtin server fs watcher");

        watcher
            .watch(Path::new("."), notify::RecursiveMode::Recursive)
            .expect("builtin server failed to watch dir");

        loop {
            if let Ok(Ok(event)) = rx.recv() {
                if let Some(event) = event.first() {
                    if event.paths.iter().any(|path| {
                        !path.to_str().unwrap().contains("dist") && !path.to_str().unwrap().contains("node_modules")
                        && !path.to_str().unwrap().contains(".notex.plugin")
                    }) {
                        if !event.kind.is_access() {
                            perform_rebuild(source.clone()).await?
                        }
                    }

                }
            }
        }
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

pub async fn create_template(main_dir: &PathBuf, dir: &Dir<'_>, t: &RenderedTemplate) -> Result<()> {
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
