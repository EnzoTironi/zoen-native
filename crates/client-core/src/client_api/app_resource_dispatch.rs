use super::{encode, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_app_resources(
        &self,
        command: &Command,
    ) -> Option<Result<String, CoreError>> {
        let result = match command {
            Command::AppResourceFind { input } => self
                .lock()
                .find_app_resource(input)
                .and_then(|value| encode(&value)),
            Command::AppResourceInstall { input } => self
                .lock()
                .install_app_resource(input.clone())
                .and_then(|value| encode(&value)),
            Command::AppResourceUpdate { input } => self
                .lock()
                .update_app_resource(input.clone())
                .and_then(|value| encode(&value)),
            Command::AppDocumentResource {
                item_id,
                expected_resource_uri,
            } => self
                .lock()
                .app_document_resource(item_id, expected_resource_uri)
                .and_then(|value| encode(&value)),
            Command::AppDocumentReadPinned {
                item_id,
                expected_resource_uri,
            } => {
                let e = self.lock();
                e.check_app_resource_pin(item_id, expected_resource_uri)
                    .and_then(|()| e.app_document_read(item_id))
                    .and_then(|value| encode(&value))
            }
            Command::AppDocumentSavePinned {
                item_id,
                expected_resource_uri,
                expected_version,
                expected_sha256,
                scene_json,
                mutation_id,
            } => {
                let mut e = self.lock();
                e.save_app_document(
                    item_id,
                    *expected_version,
                    (expected_sha256, Some(expected_resource_uri)),
                    scene_json,
                    mutation_id,
                )
                .and_then(|value| encode(&value))
            }
            Command::AppDocumentCopyPinned {
                item_id,
                expected_resource_uri,
                scene_json,
                mutation_id,
            } => {
                let mut e = self.lock();
                e.copy_app_document(
                    item_id,
                    scene_json,
                    mutation_id,
                    Some(expected_resource_uri),
                )
                .and_then(|value| encode(&value))
            }
            _ => return None,
        };
        Some(result)
    }
}
