use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;

use crate::cas::hash::{hash_to_hex, hex_to_hash};
use super::Backend;

pub struct S3Backend {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Backend {
    pub async fn new(bucket: String, prefix: Option<String>, region: Option<String>, endpoint: Option<String>) -> Result<Self> {
        let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if let Some(ref region) = region {
            config_loader = config_loader.region(aws_sdk_s3::config::Region::new(region.clone()));
        }

        let shared_config = config_loader.load().await;

        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&shared_config);

        if let Some(ref endpoint) = endpoint {
            s3_config_builder = s3_config_builder
                .endpoint_url(endpoint)
                .force_path_style(true);
        }

        let client = Client::from_conf(s3_config_builder.build());

        Ok(Self {
            client,
            bucket,
            prefix: prefix.unwrap_or_default(),
        })
    }

    fn object_key(&self, hash: &[u8; 32]) -> String {
        let hex = hash_to_hex(hash);
        let (prefix_2, rest) = hex.split_at(2);
        if self.prefix.is_empty() {
            format!("objects/{prefix_2}/{rest}")
        } else {
            format!("{}/objects/{prefix_2}/{rest}", self.prefix)
        }
    }
}

#[async_trait]
impl Backend for S3Backend {
    async fn push_chunk(&self, hash: &[u8; 32], data: &[u8]) -> Result<()> {
        let key = self.object_key(hash);
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(data.to_vec()))
            .send()
            .await
            .context("S3 put_object")?;
        Ok(())
    }

    async fn pull_chunk(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let key = self.object_key(hash);
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .context("S3 get_object")?;

        let bytes = resp.body.collect().await.context("reading S3 body")?;
        Ok(bytes.to_vec())
    }

    async fn has_chunk(&self, hash: &[u8; 32]) -> Result<bool> {
        let key = self.object_key(hash);
        match self.client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_err = e.into_service_error();
                if service_err.is_not_found() {
                    Ok(false)
                } else {
                    bail!("S3 head_object error: {service_err}")
                }
            }
        }
    }

    async fn list_chunks(&self) -> Result<Vec<[u8; 32]>> {
        let prefix = if self.prefix.is_empty() {
            "objects/".to_string()
        } else {
            format!("{}/objects/", self.prefix)
        };

        let mut hashes = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self.client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&prefix);

            if let Some(ref token) = continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req.send().await.context("S3 list_objects_v2")?;

            if let Some(contents) = resp.contents {
                for obj in contents {
                    if let Some(key) = obj.key {
                        // Extract hash from key: prefix/objects/ab/cdef...
                        let parts: Vec<&str> = key.rsplitn(3, '/').collect();
                        if parts.len() >= 2 {
                            let hex = format!("{}{}", parts[1], parts[0]);
                            if let Ok(hash) = hex_to_hash(&hex) {
                                hashes.push(hash);
                            }
                        }
                    }
                }
            }

            if resp.is_truncated == Some(true) {
                continuation_token = resp.next_continuation_token;
            } else {
                break;
            }
        }

        Ok(hashes)
    }
}
