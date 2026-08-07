//! 统一对象存储后端：`Blob` 枚举 + 方法门面（模式与 `infrastructure/cache`、`infrastructure/queue` 一致）。
//!
//! 编译期按 feature 装配（可并存，AppCtx 组装处选择用哪个变体）：
//! - `Cos`：feature `cos`（**默认**），腾讯云 COS / S3 兼容对象存储
//! - `Fs`：feature `fs`，本地文件系统（开发/单实例环境）
//! - `TestBlob`：feature `test-utils`，opendal Memory 内存后端（集成测试）
//!
//! 无 trait / 无 `dyn`：`Blob` 内部 match 派发，方法签名稳定。
//! 后端差异仅在构造（`try_new` 装配 Operator）；方法面（写 / 删 / URL 映射 / presign）三端对齐，
//! 其中 presign 仅 Cos 支持 HTTP 直传语义，Fs / TestBlob 返回 `BlobError::PresignUnsupported`。

#[cfg(feature = "cos")]
mod cos;
mod error;
#[cfg(feature = "fs")]
mod fs;
#[cfg(feature = "test-utils")]
mod memory;
mod shared;

#[cfg(feature = "test-utils")]
use crate::memory::TestBlob;
use std::{
    borrow::Cow,
    fmt::Debug,
    io::{Read, Seek},
    time::Duration,
};

use futures_util::Stream;
use rootcause::Result;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

#[cfg(feature = "cos")]
pub use cos::{Cos, CosConfig};
#[cfg(feature = "fs")]
pub use fs::{Fs, FsConfig};

#[cfg(not(any(feature = "cos", feature = "fs")))]
compile_error!("blob crate requires feature \"cos\" or \"fs\"");

/// 对象存储后端句柄：克隆共享、方法即 API。
#[derive(Clone)]
pub enum Blob {
    #[cfg(feature = "cos")]
    Cos(Cos),
    #[cfg(feature = "fs")]
    Fs(Fs),
    #[cfg(feature = "test-utils")]
    Test(TestBlob),
}

impl Debug for Blob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Blob").finish()
    }
}

impl Blob {
    /// 仅当无 `fs` 时提供：避免与其它后端同名 `try_new` 在 feature 并集下重复定义
    /// （cos 随 blob 默认特性恒在；fs 开启时 cos 分支让位，与 cache 的 pg / queue 的 pg 让位同构）。
    #[cfg(all(feature = "cos", not(feature = "fs")))]
    pub async fn try_new<'a>(config: CosConfig<'a>) -> Result<Self> {
        Ok(Self::Cos(Cos::try_new(config).await?))
    }

    #[cfg(feature = "fs")]
    pub async fn try_new<'a>(config: FsConfig<'a>) -> Result<Self> {
        Ok(Self::Fs(Fs::try_new(config).await?))
    }

    /// 测试用内存后端（opendal Memory）：不落盘、不依赖外部服务。
    #[cfg(feature = "test-utils")]
    pub async fn new_for_test() -> Result<Self> {
        Ok(Self::Test(TestBlob::new_for_test().await?))
    }

    /// 拼接公网访问 URL：已是 http(s) 绝对地址则原样返回，否则 `{domain}/{path}`。
    #[inline]
    pub fn fill_public_url<'a>(&self, path: &'a str) -> Cow<'a, str> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.fill_public_url(path),
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.fill_public_url(path),
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.fill_public_url(path),
        }
    }

    #[inline]
    pub fn fill_public_url_optional(&self, path: Option<String>) -> Option<String> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.fill_public_url_optional(path),
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.fill_public_url_optional(path),
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.fill_public_url_optional(path),
        }
    }

    /// 从公网 URL 还原存储路径：剥离 `{domain}/` 前缀；非本站 URL 原样返回。
    #[inline]
    pub fn strip_public_url_prefix<'a>(&self, path: &'a str) -> Cow<'a, str> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.strip_public_url_prefix(path),
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.strip_public_url_prefix(path),
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.strip_public_url_prefix(path),
        }
    }

    #[inline]
    pub fn strip_public_url_prefix_optional(&self, path: Option<String>) -> Option<String> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.strip_public_url_prefix_optional(path),
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.strip_public_url_prefix_optional(path),
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.strip_public_url_prefix_optional(path),
        }
    }

    /// 流式写入，返回写入字节数。fs / test 后端与 cos 行为一致。
    #[tracing::instrument(skip(stream))]
    pub async fn write_stream(
        &self,
        path: impl AsRef<str> + Debug,
        stream: impl Stream<Item = Result<ReaderStream<File>>> + Unpin,
    ) -> Result<u64> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.write_stream(path, stream).await,
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.write_stream(path, stream).await,
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.write_stream(path, stream).await,
        }
    }

    /// 整块写入（`Read + Seek`，如 `Cursor` / `NamedTempFile`），返回写入字节数。
    #[tracing::instrument(skip(reader))]
    pub async fn write(
        &self,
        path: impl AsRef<str> + Debug,
        reader: impl Read + Seek,
    ) -> Result<u64> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.write(path, reader).await,
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.write(path, reader).await,
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.write(path, reader).await,
        }
    }

    /// 批量删除（空列表安全）。
    #[tracing::instrument]
    pub async fn delete_many(&self, paths: Vec<String>) -> Result<()> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.delete_many(paths).await,
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.delete_many(paths).await,
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.delete_many(paths).await,
        }
    }

    /// 预签名视频直传 URL（仅 Cos 支持；fs / test 后端返回 `BlobError::PresignUnsupported`）。
    #[tracing::instrument]
    pub async fn presign_video_upload_url(
        &self,
        path: impl AsRef<str> + Debug,
        extension: &str,
        expire: Duration,
    ) -> Result<String> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.presign_video_upload_url(path, extension, expire).await,
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.presign_video_upload_url(path, extension, expire).await,
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.presign_video_upload_url(path, extension, expire).await,
        }
    }

    /// 预签名图片直传 URL（仅 Cos 支持；fs / test 后端返回 `BlobError::PresignUnsupported`）。
    #[tracing::instrument]
    pub async fn presign_image_upload_url(
        &self,
        path: impl AsRef<str> + Debug,
        extension: &str,
        expire: Duration,
    ) -> Result<String> {
        match self {
            #[cfg(feature = "cos")]
            Self::Cos(b) => b.presign_image_upload_url(path, extension, expire).await,
            #[cfg(feature = "fs")]
            Self::Fs(b) => b.presign_image_upload_url(path, extension, expire).await,
            #[cfg(feature = "test-utils")]
            Self::Test(b) => b.presign_image_upload_url(path, extension, expire).await,
        }
    }
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use std::io::Cursor;

    use tokio_util::io::ReaderStream;

    use super::*;

    #[tokio::test]
    async fn facade_roundtrip() {
        let blob = Blob::new_for_test().await.expect("create test blob");

        let path = "staging/test.txt";
        assert_eq!(
            blob.write(path, Cursor::new("it's working")).await.unwrap(),
            12
        );
        assert_eq!(
            blob.fill_public_url(path),
            "http://test.local/files/staging/test.txt"
        );
        assert_eq!(
            blob.strip_public_url_prefix("http://test.local/files/staging/test.txt"),
            "staging/test.txt"
        );
        assert_eq!(
            blob.fill_public_url_optional(Some(path.to_string())),
            Some("http://test.local/files/staging/test.txt".to_string())
        );
        assert_eq!(
            blob.strip_public_url_prefix_optional(Some(
                "http://test.local/files/staging/test.txt".to_string()
            )),
            Some(path.to_string())
        );

        blob.delete_many(vec![path.to_string()]).await.unwrap();
    }

    #[tokio::test]
    async fn facade_write_stream_roundtrip() {
        use std::io::Seek as _;

        let blob = Blob::new_for_test().await.expect("create test blob");

        let file = tempfile::NamedTempFile::new().expect("create temp file");
        std::io::Write::write_all(&mut file.as_file(), b"streamed data").expect("write temp");
        // 写入后 cursor 停在末尾，回卷到开头再交给 ReaderStream。
        let mut file = file.into_file();
        file.seek(std::io::SeekFrom::Start(0)).expect("rewind");
        let file = tokio::fs::File::from_std(file);
        let stream = Box::pin(futures_util::stream::once(async {
            Ok(ReaderStream::new(file))
        }));

        let path = "staging/stream.txt";
        assert_eq!(
            blob.write_stream(path, stream).await.expect("write stream"),
            13
        );
        assert_eq!(
            blob.fill_public_url(path),
            "http://test.local/files/staging/stream.txt"
        );

        blob.delete_many(vec![path.to_string()]).await.unwrap();
    }
}
