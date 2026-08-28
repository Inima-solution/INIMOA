use axum::{
    Router,
    body::to_bytes,
    extract::{Request, State},
    http::StatusCode,
    routing::any,
};
use models_properties::EntityType;
use opensearch_client::OpensearchClient;
use serde_json::json;
use tokio::sync::mpsc::UnboundedSender;

use super::process_entity_property_update;

const SAME_ID: &str = "task-property-route-01";
const MAX_CAPTURED_BODY_BYTES: usize = 4 * 1024;

async fn capture_request(
    State(sender): State<UnboundedSender<(String, String, Option<String>, Vec<u8>)>>,
    request: Request,
) -> StatusCode {
    let captured = (
        request.method().to_string(),
        request.uri().path().to_owned(),
        request.uri().query().map(str::to_owned),
        to_bytes(request.into_body(), MAX_CAPTURED_BODY_BYTES)
            .await
            .expect("capture server should read request body")
            .to_vec(),
    );

    sender
        .send(captured)
        .expect("capture receiver should remain available");

    StatusCode::OK
}

#[sqlx::test(migrations = "../../crates/macro_db_client/migrations")]
async fn task_property_update_uses_document_index_same_id_and_routing(
    pool: sqlx::Pool<sqlx::Postgres>,
) -> anyhow::Result<()> {
    let (sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let opensearch_client = OpensearchClient::new(
        format!("http://{}", listener.local_addr()?),
        "test-user".into(),
        "test-pass".into(),
    )?;
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .fallback(any(capture_request))
                .with_state(sender),
        )
        .await
        .expect("capture server should run until aborted");
    });

    let result =
        process_entity_property_update(&opensearch_client, &pool, SAME_ID, EntityType::Task).await;

    server.abort();
    result?;

    let (method, path, query, body) = requests
        .try_recv()
        .expect("Task property update should issue one OpenSearch request");
    let expected_query = format!("routing={SAME_ID}");
    assert_eq!(method, "POST");
    assert_eq!(path, format!("/documents/_update/{SAME_ID}"));
    assert_eq!(query.as_deref(), Some(expected_query.as_str()));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body)?,
        json!({ "doc": { "properties": [] } }),
    );
    assert!(
        requests.try_recv().is_err(),
        "Task property update must issue exactly one OpenSearch request"
    );

    Ok(())
}
