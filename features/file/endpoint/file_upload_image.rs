use std::io::{Cursor, Read as _, Seek as _};

use appctx::Blob;
use axum::extract::State;
use axum_typed_multipart::{FieldData, TryFromMultipart};
use file_contract::error::FileError;
use file_contract::{ensure_image_reader, staging_dated_file_path};
use image_kit::compress::{CompressOptions, ImageKit};
use serde::Serialize;
use tempfile::NamedTempFile;
use utoipa::ToSchema;
use web::extract::valid_typed_multipart::ValidTypedMultipart;
use web::response::json_response::{JsonResponse, JsonResponseType};

#[derive(TryFromMultipart, ToSchema)]
pub(crate) struct UploadImageMultipart {
    #[form_data(limit = "2MiB")]
    #[schema(value_type = Vec<u8>, format = Binary)]
    pub image: FieldData<NamedTempFile>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct UploadImageResponse {
    pub path: String,
    pub url: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/files/images",
    operation_id = "file_upload_image",
    tag = "file",
    request_body(content_type = "multipart/form-data", content = UploadImageMultipart),
    responses((status = 200, body = JsonResponse<UploadImageResponse>)),
    security(("bearerAuth" = []))
)]
#[tracing::instrument(skip(multipart))]
pub(crate) async fn handler(
    State(blob): State<Blob>,
    ValidTypedMultipart(multipart): ValidTypedMultipart<UploadImageMultipart>,
) -> JsonResponseType<UploadImageResponse> {
    let response = execute(blob, multipart).await?;
    JsonResponse::ok(response)
}

#[tracing::instrument(skip_all)]
#[inline]
async fn execute(
    blob: Blob,
    multipart: UploadImageMultipart,
) -> rootcause::Result<UploadImageResponse> {
    let UploadImageMultipart { image } = multipart;
    let Some(_file_name) = image.metadata.file_name else {
        return Err(FileError::FileNameMissing)?;
    };
    let mut file = image.contents;
    ensure_image_reader(&mut file)?;
    file.rewind()?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let compressed = tokio::task::spawn_blocking(move || {
        ImageKit::compress_to_webp(&raw, CompressOptions::default())
    })
    .await??;
    let path = staging_dated_file_path("webp");
    blob.write(&path, Cursor::new(compressed.bytes)).await?;
    let url = blob.fill_public_url(&path).to_string();
    Ok(UploadImageResponse { path, url })
}
