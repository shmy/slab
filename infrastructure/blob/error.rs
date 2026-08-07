/// 对象存储后端错误（内部基础设施：永远 500，不进 locale，对齐 `libs/image_kit`）。
#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// 预签名直传仅 Cos 支持；fs / 内存后端无 HTTP 直传语义。
    #[error("presigned upload is not supported by this blob backend")]
    PresignUnsupported,
}
