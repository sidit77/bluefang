use crate::l2cap::AVDTP_PSM;
use crate::sdp::ids::attributes::*;
use crate::sdp::ids::browse_groups::PUBLIC_BROWSE_ROOT;
use crate::sdp::ids::protocols::{AVDTP, L2CAP};
use crate::sdp::ids::service_classes::{AUDIO_SINK, ADVANCED_AUDIO_DISTRIBUTION};
use crate::sdp::{DataElement, ServiceAttribute, ServiceRecord};

pub struct A2dpSinkServiceRecord {
    handle: u32
}

impl A2dpSinkServiceRecord {
    pub fn new(handle: u32) -> Self {
        Self { handle }
    }
}

impl ServiceRecord for A2dpSinkServiceRecord {
    fn handle(&self) -> u32 {
        self.handle
    }

    // ([A2DP] Section 5.3).
    fn attributes(&self) -> Vec<ServiceAttribute> {
        let avdtp_version = 1u16 << 8 | 3u16;
        let a2dp_version = 1u16 << 8 | 3u16;
        vec![
            ServiceAttribute::new(SERVICE_RECORD_HANDLE_ID, self.handle),
            ServiceAttribute::new(BROWSE_GROUP_LIST_ID, DataElement::from_iter([PUBLIC_BROWSE_ROOT])),
            ServiceAttribute::new(SERVICE_CLASS_ID_LIST_ID, DataElement::from_iter([AUDIO_SINK])),
            ServiceAttribute::new(
                PROTOCOL_DESCRIPTOR_LIST_ID,
                DataElement::from_iter([(L2CAP, AVDTP_PSM), (AVDTP, avdtp_version)])
            ),
            ServiceAttribute::new(
                BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ID,
                DataElement::from_iter([(ADVANCED_AUDIO_DISTRIBUTION, a2dp_version)])
            ),
        ]
    }
}
