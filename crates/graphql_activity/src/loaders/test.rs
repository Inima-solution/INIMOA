use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use async_graphql::{
    Context, EmptyMutation, EmptySubscription, Object, PathSegment, Request, Schema,
};

use super::*;

#[derive(Clone, Default)]
struct TestReader {
    calls: Arc<Mutex<Vec<Vec<ActivityEdgeKey>>>>,
}

impl SoupActivityEdgeReader for TestReader {
    async fn entity_activity(
        &self,
        keys: Vec<ActivityEdgeKey>,
    ) -> HashMap<ActivityEdgeKey, ActivityEdgeLoad> {
        self.calls.lock().unwrap().push(keys.clone());
        keys.into_iter()
            .filter_map(|key| match key.entity.entity_id.as_ref() {
                "empty" => Some((key, ActivityEdgeLoad::Found(Vec::new()))),
                "failed" => Some((key, ActivityEdgeLoad::Failed)),
                "missing" => None,
                unexpected => panic!("unexpected activity key: {unexpected}"),
            })
            .collect()
    }
}

struct QueryRoot<R>(PhantomData<R>);

#[Object]
impl<R> QueryRoot<R>
where
    R: SoupActivityEdgeReader,
{
    async fn activity(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> async_graphql::Result<Option<Vec<GraphqlActivityEvent>>> {
        load_entity_activity::<R>(
            ctx,
            ActivityEdgeKey {
                entity: model_entity::EntityType::Document.with_entity_string(id),
                limit: 10,
            },
        )
        .await
        .map(Some)
    }
}

fn test_schema(
    reader: TestReader,
) -> Schema<QueryRoot<TestReader>, EmptyMutation, EmptySubscription> {
    Schema::build(QueryRoot(PhantomData), EmptyMutation, EmptySubscription)
        .data(entity_activity_loader(reader))
        .finish()
}

fn no_op_schema() -> Schema<QueryRoot<NoOpActivityReader>, EmptyMutation, EmptySubscription> {
    Schema::build(QueryRoot(PhantomData), EmptyMutation, EmptySubscription)
        .data(entity_activity_loader(NoOpActivityReader))
        .finish()
}

#[tokio::test]
async fn found_empty_and_no_op_are_empty_without_errors() {
    let empty = test_schema(TestReader::default())
        .execute("{ activity(id: \"empty\") { id } }")
        .await;
    assert!(empty.errors.is_empty());
    assert_eq!(
        empty.data.into_json().unwrap()["activity"],
        serde_json::json!([])
    );

    let no_op = no_op_schema()
        .execute("{ activity(id: \"anything\") { id } }")
        .await;
    assert!(no_op.errors.is_empty());
    assert_eq!(
        no_op.data.into_json().unwrap()["activity"],
        serde_json::json!([])
    );
}

#[tokio::test]
async fn failed_and_missing_activity_are_null_with_a_generic_error() {
    let response = test_schema(TestReader::default())
        .execute(
            "{ failed: activity(id: \"failed\") { id } missing: activity(id: \"missing\") { id } }",
        )
        .await;

    let data = response.data.into_json().unwrap();
    assert!(data["failed"].is_null());
    assert!(data["missing"].is_null());
    assert_eq!(response.errors.len(), 2);
    assert!(
        response
            .errors
            .iter()
            .all(|error| error.message == "activity is unavailable")
    );
    assert!(
        response
            .errors
            .iter()
            .any(|error| error.path == vec![PathSegment::Field("failed".to_owned())])
    );
    assert!(
        response
            .errors
            .iter()
            .any(|error| error.path == vec![PathSegment::Field("missing".to_owned())])
    );
}

#[tokio::test]
async fn mixed_aliases_keep_successful_empty_data_and_batch_once() {
    let reader = TestReader::default();
    let response = test_schema(reader.clone())
        .execute(Request::new(
            "{ good: activity(id: \"empty\") { id } bad: activity(id: \"failed\") { id } }",
        ))
        .await;

    let data = response.data.into_json().unwrap();
    assert_eq!(data["good"], serde_json::json!([]));
    assert!(data["bad"].is_null());
    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].message, "activity is unavailable");
    assert_eq!(
        response.errors[0].path,
        vec![PathSegment::Field("bad".to_owned())]
    );
    assert_eq!(reader.calls.lock().unwrap().len(), 1);
}
