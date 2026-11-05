//! Flat application request payloads for the existing and paginated FTS queries.
//! Internally tagged Command newtypes retain the exact public JSON shape.
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(super) struct SearchInput {
    pub query: String,
    pub kinds: Vec<String>,
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(super) struct SearchPageInput {
    pub query: String,
    pub kinds: Vec<String>,
    pub cursor: Option<String>,
    pub limit: u32,
    pub expected_identity_id: String,
    pub expected_device_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub period: Option<SearchPeriod>,
    #[serde(default)]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub messages: Option<MessageSearchFilter>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(super) struct SearchPeriod {
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub start_ms: i64,
    #[cfg_attr(feature = "client-types", ts(type = "number"))]
    pub end_ms: i64,
}

/// An exact identity filter, independent of participant display names.
#[derive(Debug, serde::Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(super) struct MessageSearchFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub space_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub author_id: Option<String>,
}

impl SearchPageInput {
    pub(super) fn execute(self, client: &super::Client) -> Result<String, super::CoreError> {
        super::encode(
            &client.universal_search_page_filtered(
                &self.query,
                &self.kinds,
                self.cursor.as_deref(),
                self.limit,
                (&self.expected_identity_id, &self.expected_device_id),
                store::SearchPageFilters {
                    period: self.period.map(|period| period.start_ms..period.end_ms),
                    space: self
                        .messages
                        .as_ref()
                        .and_then(|filter| filter.space_id.as_deref()),
                    author: self
                        .messages
                        .as_ref()
                        .and_then(|filter| filter.author_id.as_deref()),
                },
            )?,
        )
    }
}
