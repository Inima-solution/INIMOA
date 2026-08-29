use std::sync::Arc;

use macro_event_broker::{GlobalSpawner, KafkaEventPublisher, MacroEventBrokerService};
use projects::outbound::{PgProjectRepo, ShaCountAdapter, SqsProjectSearchIndexer};

pub type PollerEventBroker = MacroEventBrokerService<KafkaEventPublisher, GlobalSpawner>;

#[derive(Clone)]
pub struct Context {
    pub db: sqlx::Pool<sqlx::Postgres>,
    pub macro_event_broker: PollerEventBroker,
    pub sqs_client: Arc<sqs_client::SQS>,
    pub project_repo: PgProjectRepo,
    pub sha_counter: ShaCountAdapter,
    pub project_search_indexer: SqsProjectSearchIndexer<PollerEventBroker>,
}
