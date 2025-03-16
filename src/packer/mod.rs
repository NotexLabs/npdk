pub mod config_parser;

use std::io::{BufWriter, Write};
use std::path::Path;
use anyhow::{anyhow, Result};
use brotli::CompressorWriter;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::fs::File;
use tokio::io::{AsyncReadExt};
use tokio::join;
use walkdir::WalkDir;
use crate::debug_println;
use crate::packer::config_parser::Config;

pub struct Packer {
    walk_dir: WalkDir,
}

impl Packer {
    pub fn new<P: AsRef<Path>>(source: P) -> Result<Self> {
        if source.as_ref().exists() {
            return Ok(Self { walk_dir: WalkDir::new(source) })
        }
        Err(anyhow!(format!("{} does not exist", source.as_ref().display().to_string()).bright_red().bold()))
    }

    pub async fn pack(self) -> Result<()> {
        debug_println!("Packing///");
        let mut config = None;

        let mut packed_content = vec![];

        for entry in self.walk_dir {
            debug_println!("{:?}", entry);
            let entry = entry?;
            let path = entry.into_path();
            if path.to_str().unwrap().contains("@mf-types") { continue }
            if path.is_file() {
                let mut content = String::new();
                File::open(&path).await?.read_to_string(&mut content).await?;
                let file_name = path.file_name().unwrap().to_str().unwrap();
                if file_name == "plugin.conf.toml" {
                    config = Some(toml::from_str::<Config>(&content)?);
                }
                
                packed_content = vec![
                    &packed_content[..],
                    &(file_name.len() as u32).to_be_bytes()[..],
                    &(content.len() as u32).to_be_bytes()[..],
                    file_name.as_bytes(),
                    content.as_bytes(),
                ].concat::<u8>();
            }
        }

        debug_println!("Start packing");

        if let Some(config) = config {

            let (tx, mut rx) = tokio::sync::oneshot::channel();

            let compress_task = tokio::task::spawn(async move {
                let output_file = File::create(format!("{}.notex.plugin", config.package.name)).await?;
                let mut output = BufWriter::new(output_file.into_std().await);
                let mut compressor = CompressorWriter::new(
                    &mut output,
                    4096,
                    11,
                    22
                );

                compressor.write_all(&packed_content)?;
                compressor.flush()?;

                tx.send(()).unwrap();

                Ok::<(), anyhow::Error>(())
            });
            let pb_task = tokio::task::spawn(async move {
                let pb = ProgressBar::new(u64::MAX);
                pb.set_style(ProgressStyle::default_bar()
                    .template("{spinner:.green} {msg:.white.bold} [{elapsed_precise:.white.bold}]")?
                );
                pb.set_message("Packing plugin... ");

                loop {
                    if let Ok(_) = rx.try_recv() {
                        pb.finish_with_message("Packing plugin completed".bright_green().bold().to_string());
                        break;
                    }
                    pb.inc(1);
                }

                Ok::<_, anyhow::Error>(())
            });

            let _ = join!(compress_task, pb_task);

            return Ok(())
        }
        Err(anyhow!("Config not found".bright_red().bold()))
    }
}