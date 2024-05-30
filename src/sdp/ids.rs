
pub mod attributes {
    // ([Vol 3] Part B, Section 5.1.1).
    pub const SERVICE_RECORD_HANDLE_ID: u16 = 0x0000;

    // ([Vol 3] Part B, Section 5.1.2).
    pub const SERVICE_CLASS_ID_LIST_ID: u16 = 0x0001;

    // ([Vol 3] Part B, Section 5.1.3).
    pub const SERVICE_RECORD_STATE_ID: u16 = 0x0002;

    // ([Vol 3] Part B, Section 5.1.4).
    pub const SERVICE_ID_ID: u16 = 0x0003;

    // ([Vol 3] Part B, Section 5.1.5).
    pub const PROTOCOL_DESCRIPTOR_LIST_ID: u16 = 0x0004;

    // ([Vol 3] Part B, Section 5.1.6).
    pub const ADDITIONAL_PROTOCOL_DESCRIPTOR_LIST_ID: u16 = 0x0005;

    // ([Vol 3] Part B, Section 5.1.7).
    pub const BROWSE_GROUP_LIST_ID: u16 = 0x0005;

    // ([Vol 3] Part B, Section 5.1.8).
    pub const LANGUAGE_BASE__ID_LIST_ID: u16 = 0x0006;

    // ([Vol 3] Part B, Section 5.1.9).
    pub const SERVICE_INFO_TIME_TO_LIVE_ID: u16 = 0x0007;

    // ([Vol 3] Part B, Section 5.1.10).
    pub const SERVICE_AVAILABILITY_ID: u16 = 0x0008;

    // ([Vol 3] Part B, Section 5.1.11).
    pub const BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ID: u16 = 0x0009;

}

// ([Assigned Numbers] Section 3.1).
pub mod protocols {
    use crate::sdp::Uuid;

    pub const SDP: Uuid = Uuid::from_u16(0x0001);
    pub const UDP: Uuid = Uuid::from_u16(0x0002);
    pub const RFCOMM: Uuid = Uuid::from_u16(0x0003);
    pub const TCP: Uuid = Uuid::from_u16(0x0004);
    pub const TCS_BIN: Uuid = Uuid::from_u16(0x0005);
    pub const TCS_AT: Uuid = Uuid::from_u16(0x0006);
    pub const ATT: Uuid = Uuid::from_u16(0x0007);
    pub const OBEX: Uuid = Uuid::from_u16(0x0008);
    pub const IP: Uuid = Uuid::from_u16(0x0009);
    pub const FTP: Uuid = Uuid::from_u16(0x000a);
    pub const HTTP: Uuid = Uuid::from_u16(0x000c);
    pub const WSP: Uuid = Uuid::from_u16(0x000e);
    pub const BNEP: Uuid = Uuid::from_u16(0x000f);
    pub const UPNP: Uuid = Uuid::from_u16(0x0010);
    pub const HID_PROTOCOL: Uuid = Uuid::from_u16(0x0011);
    pub const HARDCOPY_CONTROL_CHANNEL: Uuid = Uuid::from_u16(0x0012);
    pub const HARDCOPY_DATA_CHANNEL: Uuid = Uuid::from_u16(0x0014);
    pub const HARDCOPY_NOTIFICATION_CHANNEL: Uuid = Uuid::from_u16(0x0016);
    pub const AVCTP: Uuid = Uuid::from_u16(0x0017);
    pub const AVDTP: Uuid = Uuid::from_u16(0x0019);
    pub const CMTP: Uuid = Uuid::from_u16(0x001b);
    pub const MCAP_CONTROL_CHANNEL: Uuid = Uuid::from_u16(0x001e);
    pub const MCAP_DATA_CHANNEL: Uuid = Uuid::from_u16(0x001f);
    pub const L2CAP: Uuid = Uuid::from_u16(0x0100);

}

// ([Assigned Numbers] Section 3.2).
pub mod browse_groups {
    use crate::sdp::Uuid;

    pub const PUBLIC_BROWSE_ROOT: Uuid = Uuid::from_u16(0x1002);
}