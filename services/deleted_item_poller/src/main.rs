#![recursion_limit = "256"]
mod config;
mod context;
mod handler;

use anyhow::Context;
use aws_lambda_events::event::eventbridge::EventBridgeEvent;
use config::Config;
use handler::handler;
use lambda_runtime::{
    Error, LambdaEvent, run, service_fn,
    tracing::{self},
};
use macro_entrypoint::MacroEntrypoint;
use macro_event_broker::{GlobalSpawner, KafkaEventPublisher};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Error> {
    MacroEntrypoint::default().init();
    tracing::trace!("initiating lambda");

    let config = Config::from_env().context("all necessary env vars should be available")?;

    tracing::trace!("initialized config");

    let macro_event_broker = context::PollerEventBroker::new(
        KafkaEventPublisher::new(config.kafka_brokers.as_ref())
            .context("failed to create kafka event publisher")?,
        GlobalSpawner,
    );

    // We should only ever need 1 connection
    let db = PgPoolOptions::new()
        .min_connections(3)
        .max_connections(3) // We want 1 db connection per dss item (document, project, chat)
        .connect(&config.database_url)
        .await
        .context("could not connect to db")?;

    let document_delete_queue = macro_queues::DocumentDeleteQueue::new();
    let chat_delete_queue = macro_queues::ChatDeleteQueue::new();
    let sqs_client = sqs_client::SQS::new(aws_sdk_sqs::Client::new(
        &macro_aws_config::get_macro_aws_config().await,
    ))
    .document_delete_queue(&document_delete_queue)
    .chat_delete_queue(&chat_delete_queue);
    let project_repo = projects::outbound::PgProjectRepo::new(db.clone());
    let redis = macro_sha_count_client::Redis::new(
        redis::Client::open(config.redis_uri.as_ref()).context("invalid REDIS_URI")?,
    );
    let sha_counter = projects::outbound::ShaCountAdapter::new(redis);
    let project_search_indexer = projects::outbound::SqsProjectSearchIndexer::new(
        Arc::new(sqs_client.clone()),
        macro_event_broker.clone(),
    );

    let ctx = context::Context {
        db,
        macro_event_broker,
        sqs_client: Arc::new(sqs_client),
        project_repo,
        sha_counter,
        project_search_indexer,
    };

    let func = service_fn(move |event: LambdaEvent<EventBridgeEvent>| {
        let ctx = ctx.clone();

        async move { handler(ctx, event).await }
    });

    run(func).await
}
