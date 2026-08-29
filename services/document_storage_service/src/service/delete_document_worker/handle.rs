use anyhow::Context;
use documents_hex::domain::ports::editing::EditingWorkerService;
use entity_access::domain::models::EntityType;
use properties::{EditReceipt, PropertiesService as _};

use crate::service::document_event_publisher::publish_document_purged_event;

use super::DeleteDocumentWorkerContext;

pub(super) enum MessageRoute {
    OwnerCleanup,
    LegacyRetention,
    PollerCandidate(chrono::DateTime<chrono::Utc>),
    AckOnly,
}

pub(super) enum PurgeRoute {
    AckOnly,
    PostCommitCleanup(macro_db_client::document::DocumentPurgeMetadata),
}

pub(super) fn parse_purge_token(token: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(token)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
}

pub(super) fn classify_message(has_owner: bool, token: Option<&str>) -> MessageRoute {
    if has_owner {
        // Existing owner messages own external cleanup; an unexpected token cannot
        // upgrade them into a database purge candidate.
        return MessageRoute::OwnerCleanup;
    }
    match token {
        None => MessageRoute::LegacyRetention,
        Some(token) => {
            parse_purge_token(token).map_or(MessageRoute::AckOnly, MessageRoute::PollerCandidate)
        }
    }
}

pub(super) fn route_purge_outcome(
    outcome: macro_db_client::document::DocumentPurgeOutcome,
) -> PurgeRoute {
    match outcome {
        macro_db_client::document::DocumentPurgeOutcome::Purged(metadata) => {
            PurgeRoute::PostCommitCleanup(metadata)
        }
        _ => PurgeRoute::AckOnly,
    }
}

#[tracing::instrument(skip(ctx, message), fields(message_id=message.message_id), err)]
pub async fn handle(
    ctx: &DeleteDocumentWorkerContext,
    message: &aws_sdk_sqs::types::Message,
) -> anyhow::Result<()> {
    tracing::debug!("processing delete document message");

    let (document_id, mut user_id, deleted_at_token) = if let Some(attributes) =
        message.message_attributes.as_ref()
    {
        let document_id = attributes
            .get("document_id")
            .map(|document_id| {
                tracing::trace!(document_id=?document_id, "found document_id in message attributes");
                document_id.string_value().unwrap_or_default().to_string()
            })
            .context("document_id should be a message attribute")?;

        let user_id = attributes.get("user_id").map(|user_id| {
            tracing::trace!(user_id=?user_id, "found user_id in message attributes");
            user_id.string_value().unwrap_or_default().to_string()
        });

        let deleted_at_token = attributes
            .get("deleted_at")
            .and_then(|deleted_at| deleted_at.string_value().map(ToString::to_string));
        (document_id, user_id, deleted_at_token)
    } else {
        ctx.worker.cleanup_message(message).await?;
        anyhow::bail!("message attributes not found")
    };

    // A tokenized ownerless message is a poller candidate. Its exact timestamp
    // means a restored, missing, or re-deleted row is acknowledged as stale
    // without touching Redis, storage, sync, editing, mentions, properties, or events.
    let message_route = classify_message(user_id.is_some(), deleted_at_token.as_deref());
    if let MessageRoute::AckOnly = message_route {
        ctx.worker.cleanup_message(message).await?;
        return Ok(());
    }
    if let MessageRoute::PollerCandidate(deleted_at) = message_route {
        let outcome =
            macro_db_client::document::purge_deleted_document(&ctx.db, &document_id, deleted_at)
                .await?;
        let metadata = match route_purge_outcome(outcome) {
            PurgeRoute::PostCommitCleanup(metadata) => metadata,
            PurgeRoute::AckOnly => {
                ctx.worker.cleanup_message(message).await?;
                return Ok(());
            }
        };
        if metadata.file_type.as_deref() == Some("docx") {
            ctx.redis_client
                .decrement_counts(&count_occurrences(metadata.bom_shas))
                .await?;
        }
        publish_document_purged_event(&ctx.macro_event_broker, &metadata.document_id)?;
        return cleanup_after_purge(ctx, message, &metadata.document_id, &metadata.owner).await;
    }

    // Legacy ownerless retention messages deliberately retain their old path.
    if matches!(message_route, MessageRoute::LegacyRetention) {
        tracing::info!(document_id=%document_id, "starting delete process for document");

        let document = macro_db_client::document::get_deleted_document_info(&ctx.db, &document_id)
            .await
            .inspect_err(
                |e| tracing::error!(error=?e, document_id=%document_id, "unable to get document"),
            )?;

        let shared_document = document.clone();
        user_id = Some(shared_document.owner.to_string());

        tracing::trace!(document_id=%document_id, user_id=?user_id, file_type=?document.file_type, "retrieved document");

        if let Some(file_type) = document.file_type
            && file_type.as_str() == "docx"
        {
            // Get the sha counts to decrement from the documents bom parts
            let bom_parts =
                macro_db_client::document::get_bom_parts(&ctx.db, &document.document_id).await?;

            // Transform bom parts into Vec<(sha, count)>
            let sha_counts = count_occurrences(
                bom_parts
                    .iter()
                    .map(|bp| bp.sha.clone())
                    .collect::<Vec<String>>(),
            );

            tracing::trace!("decrementing sha ref count");
            ctx.redis_client.decrement_counts(&sha_counts).await?;
        }

        tracing::trace!(document_id=%document.document_id, "deleting document");
        macro_db_client::document::delete_document(&ctx.db, &document.document_id).await?;
        tracing::trace!(document_id=%document.document_id, "deleted document");
    }

    let user_id = user_id.context("user_id should be some")?;
    cleanup_after_purge(ctx, message, &document_id, &user_id).await
}

async fn cleanup_after_purge(
    ctx: &DeleteDocumentWorkerContext,
    message: &aws_sdk_sqs::types::Message,
    document_id: &str,
    user_id: &str,
) -> anyhow::Result<()> {
    let _ = comms_db_client::entity_mentions::delete_entity_mentions_by_source(
        &ctx.db,
        vec![document_id.to_string()],
    )
    .await
    .inspect_err(|e| tracing::warn!(error=?e, "could not delete entity mentions for document"));
    ctx.s3_client
        .delete_document(user_id, document_id)
        .await
        .context("failed to delete files from s3")?;
    let _ = ctx
        .sync_service_client
        .delete(document_id)
        .await
        .inspect_err(|e| {
            tracing::trace!(error=?e, "could not delete file from sync service");
        });
    let _ = ctx
        .editing_worker_client
        .delete_traces(document_id)
        .await
        .inspect_err(|e| {
            tracing::trace!(error=?e, "could not delete ai edit traces");
        });
    let receipt = document_cleanup_receipt(document_id);
    let _ = ctx
        .properties_service
        .delete_entity_properties(&receipt)
        .await
        .inspect_err(|e| {
            tracing::error!(error=?e, "failed to delete entity properties");
        });
    let _ = ctx.worker.cleanup_message(message).await.inspect_err(|e| {
        tracing::error!(error=?e, "failed to cleanup message");
    });
    Ok(())
}

pub(crate) fn document_cleanup_receipt(document_id: &str) -> EditReceipt {
    EditReceipt::dangerously_assert_internal_user(document_id, EntityType::Document)
}

pub(crate) fn count_occurrences(strings: Vec<String>) -> Vec<(String, i64)> {
    use std::collections::HashMap;

    let mut counts = HashMap::new();

    for string in strings {
        *counts.entry(string).or_insert(0) += 1;
    }

    counts
        .into_iter()
        .map(|(string, count)| (string, count as i64))
        .collect()
}
