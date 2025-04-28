use anyhow::Result;
use brotli::DecompressorWriter;
use std::collections::HashMap;
use std::io::Write;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct UnPacker {
    plugin_source: String,
}

impl UnPacker {
    pub fn new(plugin_source: &str) -> Self {
        Self {
            plugin_source: plugin_source.to_string(),
        }
    }

    pub async fn unpack(self) -> Result<HashMap<String, String>> {
        let mut pack: HashMap<String, String> = HashMap::new();

        let mut raw_plugin_content = Vec::new();
        File::open(&self.plugin_source)
            .await?
            .read_to_end(&mut raw_plugin_content)
            .await?;
        let mut decompressed_plugin_content: Vec<u8> = Vec::new();
        let mut decompressor = DecompressorWriter::new(&mut decompressed_plugin_content, 4096);
        decompressor.write_all(&mut raw_plugin_content)?;
        decompressor.flush()?;

        drop(decompressor);

        while decompressed_plugin_content.len() > 0 {
            let url_size = u32::from_be_bytes(decompressed_plugin_content[0..4].try_into()?);
            let content_size = u32::from_be_bytes(decompressed_plugin_content[4..8].try_into()?);
            decompressed_plugin_content = decompressed_plugin_content[8..].to_vec();
            let url = String::from_utf8(decompressed_plugin_content[..url_size as usize].to_vec())?;
            decompressed_plugin_content = decompressed_plugin_content[url_size as usize..].to_vec();
            let content =
                String::from_utf8(decompressed_plugin_content[..content_size as usize].to_vec())?;
            decompressed_plugin_content =
                decompressed_plugin_content[content_size as usize..].to_vec();
            pack.insert(url, content);
        }

        Ok(pack)
    }
}
