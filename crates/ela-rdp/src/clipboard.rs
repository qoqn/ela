//! Text clipboard over the `cliprdr` channel. IronRDP calls these methods from inside its
//! channel processing, where nothing can be sent, so they only pass events to the Session
//! loop. Local text is read only when the remote side asks for it, on an actual paste.

use ironrdp_cliprdr::backend::CliprdrBackend;
use ironrdp_cliprdr::pdu::{
    ClipboardFormat, ClipboardFormatId, ClipboardGeneralCapabilityFlags, FileContentsRequest,
    FileContentsResponse, FormatDataRequest, FormatDataResponse, LockDataId,
};
use ironrdp_core::impl_as_any;
use tokio::sync::mpsc;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClipboardEvent {
    Ready,
    RemoteCopied,
    RemoteWantsText,
    RemoteWantsOther,
    RemoteText(String),
}

#[derive(Debug)]
pub(crate) struct ClipboardBridge {
    events: mpsc::UnboundedSender<ClipboardEvent>,
}

impl ClipboardBridge {
    pub(crate) fn new(events: mpsc::UnboundedSender<ClipboardEvent>) -> Self {
        Self { events }
    }

    fn send(&self, event: ClipboardEvent) {
        let _ = self.events.send(event);
    }
}

impl_as_any!(ClipboardBridge);

impl CliprdrBackend for ClipboardBridge {
    fn temporary_directory(&self) -> &str {
        ""
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
    }

    fn on_ready(&mut self) {}

    fn on_request_format_list(&mut self) {
        self.send(ClipboardEvent::Ready);
    }

    fn on_process_negotiated_capabilities(&mut self, _: ClipboardGeneralCapabilityFlags) {}

    fn on_remote_copy(&mut self, formats: &[ClipboardFormat]) {
        if formats
            .iter()
            .any(|format| format.id() == ClipboardFormatId::CF_UNICODETEXT)
        {
            self.send(ClipboardEvent::RemoteCopied);
        }
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        self.send(if request.format == ClipboardFormatId::CF_UNICODETEXT {
            ClipboardEvent::RemoteWantsText
        } else {
            ClipboardEvent::RemoteWantsOther
        });
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        if response.is_error() {
            return;
        }
        match response.to_unicode_string() {
            Ok(text) => self.send(ClipboardEvent::RemoteText(
                text.trim_end_matches('\0').to_owned(),
            )),
            Err(e) => tracing::debug!("remote clipboard text could not be decoded: {e}"),
        }
    }

    fn on_file_contents_request(&mut self, _: FileContentsRequest) {}

    fn on_file_contents_response(&mut self, _: FileContentsResponse<'_>) {}

    fn on_lock(&mut self, _: LockDataId) {}

    fn on_unlock(&mut self, _: LockDataId) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge() -> (ClipboardBridge, mpsc::UnboundedReceiver<ClipboardEvent>) {
        let (events, received) = mpsc::unbounded_channel();
        (ClipboardBridge::new(events), received)
    }

    #[test]
    fn only_text_copied_remotely_is_fetched() {
        let (mut bridge, mut received) = bridge();
        bridge.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_DIB)]);
        assert!(received.try_recv().is_err());
        bridge.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]);
        assert_eq!(received.try_recv().unwrap(), ClipboardEvent::RemoteCopied);
    }

    #[test]
    fn pastes_ask_for_text_or_get_refused() {
        let (mut bridge, mut received) = bridge();
        bridge.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_UNICODETEXT,
        });
        assert_eq!(
            received.try_recv().unwrap(),
            ClipboardEvent::RemoteWantsText
        );
        bridge.on_format_data_request(FormatDataRequest {
            format: ClipboardFormatId::CF_DIB,
        });
        assert_eq!(
            received.try_recv().unwrap(),
            ClipboardEvent::RemoteWantsOther
        );
    }

    #[test]
    fn remote_text_arrives_without_its_terminator() {
        let (mut bridge, mut received) = bridge();
        bridge.on_format_data_response(FormatDataResponse::new_unicode_string("héllo"));
        assert_eq!(
            received.try_recv().unwrap(),
            ClipboardEvent::RemoteText("héllo".into())
        );
    }

    #[test]
    fn a_failed_response_is_ignored() {
        let (mut bridge, mut received) = bridge();
        bridge.on_format_data_response(FormatDataResponse::new_error());
        assert!(received.try_recv().is_err());
    }
}
