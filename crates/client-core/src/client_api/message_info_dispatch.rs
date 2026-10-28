use super::{encode, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_message_info(
        &self,
        command: &Command,
    ) -> Option<Result<String, CoreError>> {
        let Command::MessageInfo { scope } = command else {
            return None;
        };
        Some(
            self.lock()
                .message_info(scope)
                .and_then(|info| encode(&info)),
        )
    }
}
