use super::{encode, Client, CoreError, Deserialize};

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ManagedAgentCommand {
    #[serde(rename = "managed_agent_start")]
    Start {
        name: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    #[serde(rename = "managed_agent_status")]
    Status {
        expected_identity_id: String,
        expected_device_id: String,
    },
    #[serde(rename = "managed_agent_approve")]
    Approve {
        operation_id: String,
        observed_offer_digest: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    #[serde(rename = "managed_agent_chat")]
    Chat {
        operation_id: String,
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    #[serde(rename = "managed_agent_discard")]
    Discard {
        operation_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
    #[serde(rename = "managed_agent_complete")]
    Complete {
        operation_id: String,
        agent_id: String,
        expected_identity_id: String,
        expected_device_id: String,
    },
}
impl ManagedAgentCommand {
    pub(super) async fn execute(self, client: &Client) -> Result<String, CoreError> {
        match self {
            Self::Start {
                name,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &client
                    .managed_enrollment_start_view(
                        &name,
                        &expected_identity_id,
                        &expected_device_id,
                    )
                    .await?,
            ),
            Self::Status {
                expected_identity_id,
                expected_device_id,
            } => encode(
                &client
                    .managed_enrollment_read_view(&expected_identity_id, &expected_device_id)
                    .await?,
            ),
            Self::Approve {
                operation_id,
                observed_offer_digest,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &client
                    .managed_enrollment_approve_view(
                        &operation_id,
                        &observed_offer_digest,
                        &expected_identity_id,
                        &expected_device_id,
                    )
                    .await?,
            ),
            Self::Chat {
                operation_id,
                agent_id,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &client
                    .chat_with_managed_agent(
                        &operation_id,
                        &agent_id,
                        &expected_identity_id,
                        &expected_device_id,
                    )
                    .await?,
            ),
            Self::Discard {
                operation_id,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &client
                    .discard_managed_enrollment(
                        &operation_id,
                        &expected_identity_id,
                        &expected_device_id,
                    )
                    .await?,
            ),
            Self::Complete {
                operation_id,
                agent_id,
                expected_identity_id,
                expected_device_id,
            } => encode(
                &client
                    .complete_managed_enrollment(
                        &operation_id,
                        &agent_id,
                        &expected_identity_id,
                        &expected_device_id,
                    )
                    .await?,
            ),
        }
    }
}
