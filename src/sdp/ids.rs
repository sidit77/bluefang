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

// ([Assigned Numbers] Section 3.3).
pub mod service_classes {
    use crate::sdp::Uuid;

    pub const SERVICE_DISCOVERY_SERVER_SERVICE_CLASS_ID: Uuid = Uuid::from_u16(0x1000);
    pub const BROWSE_GROUP_DESCRIPTOR_SERVICE_CLASS_ID: Uuid = Uuid::from_u16(0x1001);
    pub const SERIAL_PORT: Uuid = Uuid::from_u16(0x1101);
    pub const LAN_ACCESS_USING_PPP: Uuid = Uuid::from_u16(0x1102);
    pub const DIAL_UP_NETWORKING: Uuid = Uuid::from_u16(0x1103);
    pub const IRMC_SYNC: Uuid = Uuid::from_u16(0x1104);
    pub const OBEX_OBJECT_PUSH: Uuid = Uuid::from_u16(0x1105);
    pub const OBEX_FILE_TRANSFER: Uuid = Uuid::from_u16(0x1106);
    pub const IRMC_SYNC_COMMAND: Uuid = Uuid::from_u16(0x1107);
    pub const HEADSET: Uuid = Uuid::from_u16(0x1108);
    pub const CORDLESS_TELEPHONY: Uuid = Uuid::from_u16(0x1109);
    pub const AUDIO_SOURCE: Uuid = Uuid::from_u16(0x110a);
    pub const AUDIO_SINK: Uuid = Uuid::from_u16(0x110b);
    pub const AV_REMOTE_CONTROL_TARGET: Uuid = Uuid::from_u16(0x110c);
    pub const ADVANCED_AUDIO_DISTRIBUTION: Uuid = Uuid::from_u16(0x110d);
    pub const AV_REMOTE_CONTROL: Uuid = Uuid::from_u16(0x110e);
    pub const AV_REMOTE_CONTROL_CONTROLLER: Uuid = Uuid::from_u16(0x110f);
    pub const INTERCOM: Uuid = Uuid::from_u16(0x1110);
    pub const FAX: Uuid = Uuid::from_u16(0x1111);
    pub const HEADSET_AUDIO_GATEWAY: Uuid = Uuid::from_u16(0x1112);
    pub const WAP: Uuid = Uuid::from_u16(0x1113);
    pub const WAP_CLIENT: Uuid = Uuid::from_u16(0x1114);
    pub const PANU: Uuid = Uuid::from_u16(0x1115);
    pub const NAP: Uuid = Uuid::from_u16(0x1116);
    pub const GN: Uuid = Uuid::from_u16(0x1117);
    pub const DIRECT_PRINTING: Uuid = Uuid::from_u16(0x1118);
    pub const REFERENCE_PRINTING: Uuid = Uuid::from_u16(0x1119);
    pub const IMAGING: Uuid = Uuid::from_u16(0x111a);
    pub const IMAGING_RESPONDER: Uuid = Uuid::from_u16(0x111b);
    pub const IMAGING_AUTOMATIC_ARCHIVE: Uuid = Uuid::from_u16(0x111c);
    pub const IMAGING_REFERENCED_OBJECTS: Uuid = Uuid::from_u16(0x111d);
    pub const HANDS_FREE: Uuid = Uuid::from_u16(0x111e);
    pub const AG_HANDS_FREE: Uuid = Uuid::from_u16(0x111f);
    pub const DIRECT_PRINTING_REFERENCED_OBJECTS_SERVICE: Uuid = Uuid::from_u16(0x1120);
    pub const REFLECTED_UI: Uuid = Uuid::from_u16(0x1121);
    pub const BASIC_PRINTING: Uuid = Uuid::from_u16(0x1122);
    pub const PRINTING_STATUS: Uuid = Uuid::from_u16(0x1123);
    pub const HID: Uuid = Uuid::from_u16(0x1124);
    pub const HARDCOPY_CABLE_REPLACEMENT: Uuid = Uuid::from_u16(0x1125);
    pub const HCR_PRINT: Uuid = Uuid::from_u16(0x1126);
    pub const HCR_SCAN: Uuid = Uuid::from_u16(0x1127);
    pub const COMMON_ISDN_ACCESS: Uuid = Uuid::from_u16(0x1128);
    pub const SIM_ACCESS: Uuid = Uuid::from_u16(0x112d);
    pub const PHONEBOOK_ACCESS_CLIENT: Uuid = Uuid::from_u16(0x112e);
    pub const PHONEBOOK_ACCESS_SERVER: Uuid = Uuid::from_u16(0x112f);
    pub const PHONEBOOK_ACCESS_PROFILE: Uuid = Uuid::from_u16(0x1130);
    pub const HEADSET_HS: Uuid = Uuid::from_u16(0x1131);
    pub const MESSAGE_ACCESS_SERVER: Uuid = Uuid::from_u16(0x1132);
    pub const MESSAGE_NOTIFICATION_SERVER: Uuid = Uuid::from_u16(0x1133);
    pub const MESSAGE_ACCESS_PROFILE: Uuid = Uuid::from_u16(0x1134);
    pub const GNSS: Uuid = Uuid::from_u16(0x1135);
    pub const GNSS_SERVER: Uuid = Uuid::from_u16(0x1136);
    pub const THREED_DISPLAY: Uuid = Uuid::from_u16(0x1137);
    pub const THREED_GLASSES: Uuid = Uuid::from_u16(0x1138);
    pub const THREED_SYNCH_PROFILE: Uuid = Uuid::from_u16(0x1139);
    pub const MULTI_PROFILE_SPECIFICATION: Uuid = Uuid::from_u16(0x113a);
    pub const MPS: Uuid = Uuid::from_u16(0x113b);
    pub const CTN_ACCESS_SERVICE: Uuid = Uuid::from_u16(0x113c);
    pub const CTN_NOTIFICATION_SERVICE: Uuid = Uuid::from_u16(0x113d);
    pub const CALENDAR_TASKS_NOTES_PROFILE: Uuid = Uuid::from_u16(0x113e);
    pub const PN_P_INFORMATION: Uuid = Uuid::from_u16(0x1200);
    pub const GENERIC_NETWORKING: Uuid = Uuid::from_u16(0x1201);
    pub const GENERIC_FILE_TRANSFER: Uuid = Uuid::from_u16(0x1202);
    pub const GENERIC_AUDIO: Uuid = Uuid::from_u16(0x1203);
    pub const GENERIC_TELEPHONY: Uuid = Uuid::from_u16(0x1204);
    pub const UPNP_SERVICE: Uuid = Uuid::from_u16(0x1205);
    pub const UPNP_IP_SERVICE: Uuid = Uuid::from_u16(0x1206);
    pub const ESDP_UPNP_IP_PAN: Uuid = Uuid::from_u16(0x1300);
    pub const ESDP_UPNP_IP_LAP: Uuid = Uuid::from_u16(0x1301);
    pub const ESDP_UPNP_CAP: Uuid = Uuid::from_u16(0x1302);
    pub const VIDEO_SOURCE: Uuid = Uuid::from_u16(0x1303);
    pub const VIDEO_SINK: Uuid = Uuid::from_u16(0x1304);
    pub const VIDEO_DISTRIBUTION: Uuid = Uuid::from_u16(0x1305);
    pub const HDP: Uuid = Uuid::from_u16(0x1400);
    pub const HDP_SOURCE: Uuid = Uuid::from_u16(0x1401);
    pub const HDP_SINK: Uuid = Uuid::from_u16(0x1402);
}