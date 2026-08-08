#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Timeout")]
    Timeout,
}
