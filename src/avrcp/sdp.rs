use bitflags::bitflags;

use crate::l2cap::AVCTP_PSM;
use crate::sdp::ids::attributes::{
    BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ID, BROWSE_GROUP_LIST_ID, PROTOCOL_DESCRIPTOR_LIST_ID, SERVICE_CLASS_ID_LIST_ID, SERVICE_RECORD_HANDLE_ID
};
use crate::sdp::ids::browse_groups::PUBLIC_BROWSE_ROOT;
use crate::sdp::ids::protocols::{AVCTP, L2CAP};
use crate::sdp::{DataElement, ServiceAttribute, ServiceRecord};
use crate::sdp::ids::service_classes::{AV_REMOTE_CONTROL, AV_REMOTE_CONTROL_CONTROLLER, AV_REMOTE_CONTROL_TARGET};


// ([Assigned Numbers] Section 5.1.2).
const SUPPORTED_FEATURES_ID: u16 = 0x0311;

#[derive(Debug)]
pub struct AvrcpControllerServiceRecord {
    handle: u32
}

impl AvrcpControllerServiceRecord {
    pub fn new(handle: u32) -> Self {
        Self { handle }
    }
}

impl ServiceRecord for AvrcpControllerServiceRecord {
    fn handle(&self) -> u32 {
        self.handle
    }

    // ([AVRCP] Section 8).
    fn attributes(&self) -> Vec<ServiceAttribute> {
        let avctp_version = 1u16 << 8 | 4u16;
        let avcrp_version = 1u16 << 8 | 6u16;

        vec![
            ServiceAttribute::new(SERVICE_RECORD_HANDLE_ID, self.handle),
            ServiceAttribute::new(BROWSE_GROUP_LIST_ID, DataElement::from_iter([PUBLIC_BROWSE_ROOT])),
            ServiceAttribute::new(
                SERVICE_CLASS_ID_LIST_ID,
                DataElement::from_iter([AV_REMOTE_CONTROL, AV_REMOTE_CONTROL_CONTROLLER])
            ),
            ServiceAttribute::new(
                PROTOCOL_DESCRIPTOR_LIST_ID,
                DataElement::from_iter([(L2CAP, AVCTP_PSM), (AVCTP, avctp_version)])
            ),
            ServiceAttribute::new(
                BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ID,
                DataElement::from_iter([(AV_REMOTE_CONTROL, avcrp_version)])
            ),
            ServiceAttribute::new(SUPPORTED_FEATURES_ID, SupportedControllerFeatures::CATEGORY_1),
        ]
    }
}

// ([AVRCP] Section 8).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct SupportedControllerFeatures: u16 {
        const CATEGORY_1 = 1 << 0;
        const CATEGORY_2 = 1 << 1;
        const CATEGORY_3 = 1 << 2;
        const CATEGORY_4 = 1 << 3;
        const BROWSING = 1 << 6;
        const COVER_ART_IMAGE_PROPERTIES = 1 << 7;
        const COVER_ART_IMAGE = 1 << 8;
        const COVER_ART_LINKED_THUNBNAIL = 1 << 9;
    }
}

impl From<SupportedControllerFeatures> for DataElement {
    fn from(features: SupportedControllerFeatures) -> Self {
        DataElement::from(features.bits())
    }
}

#[derive(Debug)]
pub struct AvrcpTargetServiceRecord {
    handle: u32
}

impl AvrcpTargetServiceRecord {
    pub fn new(handle: u32) -> Self {
        Self { handle }
    }
}

impl ServiceRecord for AvrcpTargetServiceRecord {
    fn handle(&self) -> u32 {
        self.handle
    }

    // ([AVRCP] Section 8).
    fn attributes(&self) -> Vec<ServiceAttribute> {
        let avctp_version = 1u16 << 8 | 4u16;
        let avcrp_version = 1u16 << 8 | 6u16;

        vec![
            ServiceAttribute::new(SERVICE_RECORD_HANDLE_ID, self.handle),
            ServiceAttribute::new(BROWSE_GROUP_LIST_ID, DataElement::from_iter([PUBLIC_BROWSE_ROOT])),
            ServiceAttribute::new(SERVICE_CLASS_ID_LIST_ID, DataElement::from_iter([AV_REMOTE_CONTROL_TARGET])),
            ServiceAttribute::new(
                PROTOCOL_DESCRIPTOR_LIST_ID,
                DataElement::from_iter([(L2CAP, AVCTP_PSM), (AVCTP, avctp_version)])
            ),
            ServiceAttribute::new(
                BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ID,
                DataElement::from_iter([(AV_REMOTE_CONTROL, avcrp_version)])
            ),
            ServiceAttribute::new(SUPPORTED_FEATURES_ID, SupportedTargetFeatures::CATEGORY_2),
        ]
    }
}

// ([AVRCP] Section 8).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct SupportedTargetFeatures: u16 {
        const CATEGORY_1 = 1 << 0;
        const CATEGORY_2 = 1 << 1;
        const CATEGORY_3 = 1 << 2;
        const CATEGORY_4 = 1 << 3;
        const SETTINGS = 1 << 4;
        const GROUP_NAVIGATION = 1 << 5;
        const BROWSING = 1 << 6;
        const MULTIPLE_PLAYER = 1 << 7;
        const COVER_ART = 1 << 8;
    }
}

impl From<SupportedTargetFeatures> for DataElement {
    fn from(features: SupportedTargetFeatures) -> Self {
        DataElement::from(features.bits())
    }
}
