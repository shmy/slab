use futures_util::{SinkExt as _, Stream, StreamExt as _};
use opendal::{
    DeleteInput, IntoDeleteInput, Operator, layers::LoggingLayer, options::WriteOptions, services,
};
use rootcause::Result;
use std::{
    borrow::Cow,
    fmt::Debug,
    io::{Read, Seek},
    time::Duration,
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

pub struct BlobConfig<'a> {
    pub endpoint: &'a str,
    pub domain: &'a str,
    pub bucket: &'a str,
    pub secret_id: &'a str,
    pub secret_key: &'a str,
}

#[derive(Clone)]
pub struct Blob {
    op: Operator,
    domain: String,
}

impl Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob").finish()
    }
}

impl Blob {
    pub async fn try_new<'a>(config: BlobConfig<'a>) -> Result<Self> {
        let bucket = config.bucket;

        let op = {
            let builder = services::Cos::default()
                .endpoint(config.endpoint)
                .bucket(bucket)
                .secret_id(config.secret_id)
                .secret_key(config.secret_key);
            Operator::new(builder)?.layer(LoggingLayer::default())
        };
        let cloned_op = op.clone();
        let bucket = bucket.to_string();
        tokio::spawn(async move {
            let result = cloned_op.check().await;
            if let Err(e) = result {
                tracing::error!("Blob bucket: {} unavailable: {}", bucket, e);
            } else {
                tracing::info!("Blob bucket: {} available", bucket);
            }
        });
        Ok(Self {
            op,
            domain: config.domain.to_string(),
        })
    }

    #[cfg(feature = "test-utils")]
    pub async fn new_for_test() -> Result<Self> {
        let op = Operator::new(services::Memory::default())?.layer(LoggingLayer::default());
        op.check().await?;
        Ok(Self {
            op,
            domain: "http://test.local/files".to_string(),
        })
    }
}

impl Blob {
    #[inline]
    pub fn fill_public_url<'a>(&self, path: &'a str) -> Cow<'a, str> {
        if path.starts_with("http://") || path.starts_with("https://") {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(format!("{}/{}", self.domain, path))
        }
    }

    #[inline]
    pub fn fill_public_url_optional(&self, path: Option<String>) -> Option<String> {
        path.filter(|p| !p.is_empty())
            .map(|p| self.fill_public_url(&p).into_owned())
    }

    #[inline]
    pub fn extra_path<'a>(&self, path: &'a str) -> Cow<'a, str> {
        let prefix = format!("{}/", self.domain);
        if let Some(stripped) = path.strip_prefix(&prefix) {
            Cow::Owned(stripped.to_string())
        } else {
            Cow::Borrowed(path)
        }
    }

    #[inline]
    pub fn extra_path_optional(&self, path: Option<String>) -> Option<String> {
        path.as_deref().map(|p| self.extra_path(p).into_owned())
    }
}

impl Blob {
    #[tracing::instrument(skip(stream))]
    pub async fn write_stream(
        &self,
        path: impl AsRef<str> + Debug,
        mut stream: impl Stream<Item = Result<ReaderStream<File>>> + Unpin,
    ) -> Result<u64> {
        let writer = self.op.writer_with(path.as_ref()).concurrent(8).await?;
        let mut sink = writer.into_bytes_sink();
        let mut total_size: u64 = 0;

        while let Some(rs) = stream.next().await {
            let mut rs = rs?;
            while let Some(chunk) = rs.next().await {
                let chunk = chunk?;
                total_size += chunk.len() as u64;
                sink.send(chunk).await?;
            }
        }

        sink.close().await?;
        Ok(total_size)
    }

    #[tracing::instrument(skip(reader))]
    pub async fn write(
        &self,
        path: impl AsRef<str> + Debug,
        mut reader: impl Read + Seek,
    ) -> Result<u64> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let total_size = buf.len() as u64;
        let _ = self.op.write(path.as_ref(), buf).await?;
        Ok(total_size)
    }

    #[tracing::instrument]
    pub async fn delete_many(&self, paths: Vec<String>) -> Result<()> {
        let items: Vec<DeleteInput> = paths
            .into_iter()
            .map(IntoDeleteInput::into_delete_input)
            .collect();
        self.op.delete_iter(items).await?;
        Ok(())
    }

    #[tracing::instrument]
    pub async fn presign_video_upload_url(
        &self,
        path: impl AsRef<str> + Debug,
        extension: &str,
        expire: Duration,
    ) -> Result<String> {
        let req = self
            .op
            .presign_write_options(
                path.as_ref(),
                expire,
                WriteOptions {
                    content_type: Some(format!("video/{}", extension)),
                    ..Default::default()
                },
            )
            .await?;
        Ok(req.uri().to_string())
    }

    #[tracing::instrument]
    pub async fn presign_image_upload_url(
        &self,
        path: impl AsRef<str> + Debug,
        extension: &str,
        expire: Duration,
    ) -> Result<String> {
        let req = self
            .op
            .presign_write_options(
                path.as_ref(),
                expire,
                WriteOptions {
                    content_type: Some(format!("image/{}", extension)),
                    ..Default::default()
                },
            )
            .await?;
        Ok(req.uri().to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{env::var, io::Cursor};

    use super::*;
    #[tokio::test]
    async fn test_upload() {
        dotenvy::dotenv().ok();
        let config = BlobConfig {
            endpoint: &var("S3_ENDPOINT").unwrap(),
            domain: &var("S3_DOMAIN").unwrap(),
            bucket: &var("S3_BUCKET").unwrap(),
            secret_id: &var("S3_SECRET_ID").unwrap(),
            secret_key: &var("S3_SECRET_KEY").unwrap(),
        };
        let s3 = Blob::try_new(config).await.unwrap();
        let cu = Cursor::new("it's working");
        let _ = s3.write("test.txt", cu).await.unwrap();
        s3.delete_many(vec!["test.txt".to_string()]).await.unwrap();
    }

    #[tokio::test]
    async fn test_presign_video_upload_url() {
        dotenvy::dotenv().ok();
        let config = BlobConfig {
            endpoint: &var("S3_ENDPOINT").unwrap(),
            domain: &var("S3_DOMAIN").unwrap(),
            bucket: &var("S3_BUCKET").unwrap(),
            secret_id: &var("S3_SECRET_ID").unwrap(),
            secret_key: &var("S3_SECRET_KEY").unwrap(),
        };
        let s3 = Blob::try_new(config).await.unwrap();
        let url = s3
            .presign_video_upload_url("2024/12/test.mp4", "mp4", Duration::from_secs(600))
            .await
            .unwrap();
        println!("Presigned URL: {}", url);
    }

    #[tokio::test]
    async fn test_fill_public_url() {
        dotenvy::dotenv().ok();
        let public_domain = var("S3_DOMAIN").unwrap();
        let config = BlobConfig {
            endpoint: &var("S3_ENDPOINT").unwrap(),
            domain: &var("S3_DOMAIN").unwrap(),
            bucket: &var("S3_BUCKET").unwrap(),
            secret_id: &var("S3_SECRET_ID").unwrap(),
            secret_key: &var("S3_SECRET_KEY").unwrap(),
        };
        let s3 = Blob::try_new(config).await.unwrap();
        assert_eq!(
            s3.fill_public_url("https://example.com/test.txt"),
            "https://example.com/test.txt"
        );
        assert_eq!(
            s3.fill_public_url("https://www.baidu.com/img/flexible/logo/pc/peak-result.png"),
            "https://www.baidu.com/img/flexible/logo/pc/peak-result.png"
        );

        let link = s3.fill_public_url("assets/icons/logoT0.png");
        println!("Link: {}", link);
        assert_eq!(
            link,
            format!("{}/{}", public_domain, "assets/icons/logoT0.png")
        );
    }

    #[tokio::test]
    async fn test_extra_path() {
        dotenvy::dotenv().ok();
        let public_domain = var("S3_DOMAIN").unwrap();
        let config = BlobConfig {
            endpoint: &var("S3_ENDPOINT").unwrap(),
            domain: &var("S3_DOMAIN").unwrap(),
            bucket: &var("S3_BUCKET").unwrap(),
            secret_id: &var("S3_SECRET_ID").unwrap(),
            secret_key: &var("S3_SECRET_KEY").unwrap(),
        };
        let s3 = Blob::try_new(config).await.unwrap();
        assert_eq!(
            s3.extra_path("https://www.baidu.com/img/flexible/logo/pc/peak-result.png"),
            "https://www.baidu.com/img/flexible/logo/pc/peak-result.png"
        );
        assert_eq!(
            s3.extra_path(&format!("{}/assets/icons/logoT0.png", public_domain)),
            "assets/icons/logoT0.png"
        );
        assert_eq!(
            s3.extra_path("assets/icons/logoT0.png"),
            "assets/icons/logoT0.png"
        );
    }
}
