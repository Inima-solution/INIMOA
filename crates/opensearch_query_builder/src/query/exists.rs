use std::borrow::Cow;

use serde_json::{Map, Value};

use crate::{QueryType, ToOpenSearchJson};

/// An OpenSearch `exists` query for documents with a mapped field value.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExistsQuery<'a> {
    /// The field that must have a value.
    pub field: Cow<'a, str>,
}

impl<'a> ExistsQuery<'a> {
    /// Creates an exists query for `field`.
    pub fn new(field: impl Into<Cow<'a, str>>) -> Self {
        Self {
            field: field.into(),
        }
    }

    /// Converts this query to an owned query.
    pub fn to_owned(&self) -> ExistsQuery<'static> {
        ExistsQuery {
            field: Cow::Owned(self.field.to_string()),
        }
    }
}

impl<'a> From<ExistsQuery<'a>> for QueryType<'a> {
    fn from(query: ExistsQuery<'a>) -> Self {
        Self::Exists(query)
    }
}

impl<'a> ToOpenSearchJson for ExistsQuery<'a> {
    fn to_json(&self) -> Value {
        let mut exists = Map::new();
        exists.insert("field".to_string(), Value::String(self.field.to_string()));
        let mut result = Map::new();
        result.insert("exists".to_string(), Value::Object(exists));
        Value::Object(result)
    }
}

#[cfg(test)]
mod test;
