#[cfg(test)]
mod test;

use crate::context::{self};
use anyhow::Context;
use aws_lambda_events::eventbridge::EventBridgeEvent;
use chat::domain::events::{ChatMacroEvent, ChatPermanentlyDeletedMetadata};
use futures::future::join_all;
use lambda_runtime::{
    Error, LambdaEvent,
    tracing::{self},
};
use macro_event_broker::MacroEventBroker;
use projects::domain::service::{ProjectPurgeCoordinator, ProjectPurgeOutcome};
use std::future::Future;

#[tracing::instrument(skip(ctx, _event), err)]
pub async fn handler(
    ctx: context::Context,
    _event: LambdaEvent<EventBridgeEvent>,
) -> Result<(), Error> {
    let _ = tokio::try_join!(
        handle_chats(&ctx),
        handle_documents(&ctx),
        handle_projects(&ctx)
    )?;

    Ok(())
}

#[tracing::instrument(skip(event_broker, chat_ids), err)]
async fn publish_chat_purge_events<B: MacroEventBroker>(
    event_broker: &B,
    chat_ids: &[String],
) -> anyhow::Result<()> {
    let events = chat_ids
        .iter()
        .map(|chat_id| {
            ChatMacroEvent::permanently_deleted(ChatPermanentlyDeletedMetadata {
                chat_id: chat_id.clone(),
                actor_user_id: None,
                project_id: None,
            })
        })
        .collect::<Vec<_>>();

    let publications = events
        .iter()
        .map(|event| event_broker.send_event(event))
        .collect::<Vec<_>>();
    let publication_results = join_all(publications.into_iter().map(|publication| async move {
        let handle = publication.context("failed to enqueue chat purge event")?;
        handle
            .await
            .context("chat purge event publication task failed")?
            .context("failed to publish chat purge event")
    }))
    .await;

    for result in publication_results {
        result?;
    }

    Ok(())
}

#[tracing::instrument(skip(ctx), err)]
async fn handle_projects(ctx: &context::Context) -> anyhow::Result<()> {
    let date = chrono::Utc::now().naive_utc() - chrono::Duration::days(30);

    let projects_to_delete =
        macro_db_client::projects::get_projects_to_delete(&ctx.db, &date).await?;

    if projects_to_delete.is_empty() {
        tracing::info!("no projects to delete");
        return Ok(());
    }

    let coordinator = ProjectPurgeCoordinator::new(
        &ctx.project_repo,
        &ctx.sha_counter,
        &ctx.project_search_indexer,
        &ctx.macro_event_broker,
    );
    let coordinator_ref = &coordinator;
    process_project_candidates(projects_to_delete, move |candidate| {
        let coordinator = coordinator_ref;
        async move {
            coordinator
                .purge(&candidate.project_id, candidate.deleted_at, None)
                .await
        }
    })
    .await?;

    Ok(())
}

async fn process_project_candidates<F, Fut>(
    candidates: Vec<macro_db_client::projects::ProjectToDelete>,
    mut purge: F,
) -> anyhow::Result<()>
where
    F: FnMut(macro_db_client::projects::ProjectToDelete) -> Fut,
    Fut: Future<Output = Result<ProjectPurgeOutcome, projects::domain::models::ProjectError>>,
{
    for candidate in candidates {
        let project_id = candidate.project_id.clone();
        match purge(candidate)
            .await
            .context("unable to purge project candidate")?
        {
            ProjectPurgeOutcome::Purged(_) => {}
            ProjectPurgeOutcome::StaleOrUnavailable => {
                tracing::debug!(%project_id, "project purge candidate is stale");
            }
        }
    }
    Ok(())
}

#[tracing::instrument(skip(ctx), err)]
async fn handle_chats(ctx: &context::Context) -> anyhow::Result<()> {
    let date = chrono::Utc::now().naive_utc() - chrono::Duration::days(30);

    let chats_to_delete = macro_db_client::chat::get_chats_to_delete(&ctx.db, &date).await?;

    if chats_to_delete.is_empty() {
        tracing::info!("no chats to delete");
        return Ok(());
    }

    tracing::debug!(chats_to_delete=?chats_to_delete, "chats to delete");

    publish_chat_purge_events(&ctx.macro_event_broker, &chats_to_delete)
        .await
        .context("unable to publish chat purge events")?;

    ctx.sqs_client
        .bulk_enqueue_chat_delete(chats_to_delete)
        .await?;

    Ok(())
}

#[tracing::instrument(skip(ctx), err)]
async fn handle_documents(ctx: &context::Context) -> anyhow::Result<()> {
    let date = chrono::Utc::now().naive_utc() - chrono::Duration::days(30);

    let documents_to_delete =
        macro_db_client::document::get_all_documents::get_documents_to_delete(&ctx.db, &date)
            .await?;

    if documents_to_delete.is_empty() {
        tracing::info!("no documents to delete");
        return Ok(());
    }

    tracing::debug!(documents_to_delete=?documents_to_delete, "documents to delete");

    ctx.sqs_client
        .bulk_enqueue_document_purge_candidates(document_purge_queue_entries(documents_to_delete))
        .await?;

    Ok(())
}

fn document_purge_queue_entries(
    candidates: Vec<macro_db_client::document::get_all_documents::DocumentPurgeCandidate>,
) -> Vec<(String, String)> {
    candidates
        .into_iter()
        .map(|candidate| (candidate.document_id, candidate.deleted_at.to_rfc3339()))
        .collect()
}
