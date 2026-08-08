use appctx::AppCtx;
use event_bus::Subscriber;
use identity_contract::events::AccountLoggedInEvent;
use rootcause::Result;
use shared_contract::event::Event as _;

pub struct AccountLoggedInSubscriber;

impl Subscriber<AppCtx> for AccountLoggedInSubscriber {
    fn topic(&self) -> &'static str {
        AccountLoggedInEvent::TOPIC
    }

    #[tracing::instrument(skip_all)]
    fn handle<'a>(
        &'a self,
        _ctx: &'a AppCtx,
        payload: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let event: AccountLoggedInEvent = serde_json::from_value(payload)?;
            tracing::info!(%event.id, "identity: account.logged_in event received");
            Ok(())
        })
    }
}
