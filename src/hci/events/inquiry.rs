use smallvec::SmallVec;
use tracing::debug;
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::consts::{ClassOfDevice, RemoteAddr, Status};
use crate::hci::Error;
use crate::hci::events::{EventRouter, InquiryEvent};

impl EventRouter {

    pub fn handle_inquiry_event(&self, event: InquiryEvent, mut payload: ReceiveBuffer) -> Result<(), Error> {
        match event {
            InquiryEvent::Complete => {
                // ([Vol 4] Part E, Section 7.7.1).
                let status = Status::from(payload.u8()?);
                payload.finish()?;
                debug!("Inquiry complete: {}", status);
            },
            InquiryEvent::Result => {
                // ([Vol 4] Part E, Section 7.7.2).
                let count = payload.u8()? as usize;
                let addr: SmallVec<[RemoteAddr; 2]> = (0..count)
                    .map(|_| payload.bytes().map(RemoteAddr::from))
                    .collect::<Result<_, _>>()?;
                payload.skip(count * 3); // repetition mode
                let classes: SmallVec<[ClassOfDevice; 2]> = (0..count)
                    .map(|_| payload
                        .u24()
                        .map(ClassOfDevice::from))
                    .collect::<Result<_, _>>()?;
                payload.skip(count * 2); // clock offset
                payload.finish()?;

                for i in 0..count {
                    debug!("Inquiry result: {} {:?}", addr[i], classes[i]);
                }
            }
        }
        Ok(())
    }

}