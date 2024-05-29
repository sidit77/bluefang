use crate::sdp::{DataElement, ServiceAttribute, ServiceRecord};
use crate::sdp::ids::attributes::{BROWSE_GROUP_LIST_ID, SERVICE_RECORD_HANDLE_ID};
use crate::sdp::ids::browse_groups::PUBLIC_BROWSE_ROOT;

#[derive(Debug)]
pub struct AvrcpServiceRecord {
    handle: u32,
}

impl AvrcpServiceRecord {
    pub fn new(handle: u32) -> Self {
        Self { handle }
    }
}

impl ServiceRecord for AvrcpServiceRecord {
    fn handle(&self) -> u32 {
        self.handle
    }

    // ([AVRCP] Section 8).
    fn attributes(&self) -> Vec<ServiceAttribute> {
        vec![
            ServiceAttribute::new(SERVICE_RECORD_HANDLE_ID, self.handle),
            ServiceAttribute::new(BROWSE_GROUP_LIST_ID, DataElement::from_iter([
                PUBLIC_BROWSE_ROOT,
            ])),

            //ServiceAttribute::new(SERVICE_CLASS_ID_LIST_ID, DataElement::from_iter([
            //    AUDIO_SINK_SERVICE,
            //])),
            //ServiceAttribute::new(PROTOCOL_DESCRIPTOR_LIST_ID, DataElement::from_iter([
            //    (L2CAP, AVDTP_PSM),
            //    (AVDTP, avdtp_version)
            //])),
            //ServiceAttribute::new(BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ID, DataElement::from_iter([
            //    (ADVANCED_AUDIO_DISTRIBUTION_SERVICE, a2dp_version)
            //])),
        ]
    }
}