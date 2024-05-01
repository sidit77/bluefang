use smallvec::SmallVec;
use tracing::debug;
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::consts::{ClassOfDevice, RemoteAddr, Status};
use crate::hci::Error;
use crate::hci::events::{InquiryEvent};

impl EventRouter {

    pub fn handle_inquiry_event(&self, event: InquiryEvent, mut payload: ReceiveBuffer) -> Result<(), Error> {
        match event {

        }
        Ok(())
    }

}