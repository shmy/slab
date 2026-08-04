use identity_contract::events::AccountCreatedEvent;
use queue::QueueHandler;
use rootcause::Result;
use shared_contract::event::Event as _;
use sqlx::PgConnection;

pub struct AccountCreatedHandler;

impl QueueHandler for AccountCreatedHandler {
    fn topic(&self) -> &'static str {
        AccountCreatedEvent::TOPIC
    }

    #[tracing::instrument(skip_all)]
    fn handle<'a>(
        &'a self,
        _conn: &'a mut PgConnection,
        payload: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let event: AccountCreatedEvent = serde_json::from_value(payload)?;
            tracing::info!(%event.id, "identity: account.created event received");
            Ok(())
        })
    }
}
