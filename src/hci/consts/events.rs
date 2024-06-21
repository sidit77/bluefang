use std::fmt::{Display, Formatter};

use enum_iterator::Sequence;
use instructor::{Exstruct, Instruct};

/// HCI event codes ([Vol 4] Part E, Section 7.7).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Exstruct, Sequence)]
#[repr(u8)]
pub enum EventCode {
    InquiryComplete = 0x01,
    InquiryResult = 0x02,
    ConnectionComplete = 0x03,
    ConnectionRequest = 0x04,
    DisconnectionComplete = 0x05,
    AuthenticationComplete = 0x06,
    RemoteNameRequestComplete = 0x07,
    EncryptionChange = 0x08,
    ChangeConnectionLinkKeyComplete = 0x09,
    LinkKeyTypeChanged = 0x0A,
    ReadRemoteSupportedFeaturesComplete = 0x0B,
    ReadRemoteVersionInformationComplete = 0x0C,
    QosSetupComplete = 0x0D,
    CommandComplete = 0x0E,
    CommandStatus = 0x0F,
    HardwareError = 0x10,
    FlushOccurred = 0x11,
    RoleChange = 0x12,
    NumberOfCompletedPackets = 0x13,
    ModeChange = 0x14,
    ReturnLinkKeys = 0x15,
    PinCodeRequest = 0x16,
    LinkKeyRequest = 0x17,
    LinkKeyNotification = 0x18,
    LoopbackCommand = 0x19,
    DataBufferOverflow = 0x1A,
    MaxSlotsChange = 0x1B,
    ReadClockOffsetComplete = 0x1C,
    ConnectionPacketTypeChanged = 0x1D,
    QosViolation = 0x1E,
    PageScanModeChange = 0x1F,
    PageScanRepetitionModeChange = 0x20,
    FlowSpecificationComplete = 0x21,
    InquiryResultWithRssi = 0x22,
    ReadRemoteExtendedFeaturesComplete = 0x23,
    SynchronousConnectionComplete = 0x2C,
    SynchronousConnectionChanged = 0x2D,
    SniffSubrating = 0x2E,
    ExtendedInquiryResult = 0x2F,
    EncryptionKeyRefreshComplete = 0x30,
    IoCapabilityRequest = 0x31,
    IoCapabilityResponse = 0x32,
    UserConfirmationRequest = 0x33,
    UserPasskeyRequest = 0x34,
    RemoteOobDataRequest = 0x35,
    SimplePairingComplete = 0x36,
    LinkSupervisionTimeoutChanged = 0x38,
    EnhancedFlushComplete = 0x39,
    UserPasskeyNotification = 0x3B,
    KeypressNotification = 0x3C,
    RemoteHostSupportedFeaturesNotification = 0x3D,
    NumberOfCompletedDataBlocks = 0x48,
    LeMeta = 0x3E,
    TriggeredClockCapture = 0x4E,
    SynchronizationTrainComplete = 0x4F,
    SynchronizationTrainReceived = 0x50,
    ConnectionlessSlaveBroadcastReceive = 0x51,
    ConnectionlessSlaveBroadcastTimeout = 0x52,
    TruncatedPageComplete = 0x53,
    PeripheralPageResponseTimeout = 0x54,
    ConnectionlessSlaveBroadcastChannelMapChange = 0x55,
    InquiryResponseNotification = 0x56,
    AuthenticatedPayloadTimeoutExpired = 0x57,
    SamStatusChange = 0x58,
    EncryptionChangeV2 = 0x59,
    Vendor = 0xFF
}

/// HCI status codes ([Vol 1] Part F, Section 1.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Instruct, Exstruct)]
#[non_exhaustive]
#[repr(u8)]
pub enum Status {
    Success = 0x00,
    UnknownCommand = 0x01,
    UnknownConnectionIdentifier = 0x02,
    HardwareFailure = 0x03,
    PageTimeout = 0x04,
    AuthenticationFailure = 0x05,
    PinOrKeyMissing = 0x06,
    MemoryCapacityExceeded = 0x07,
    ConnectionTimeout = 0x08,
    ConnectionLimitExceeded = 0x09,
    SynchronousConnectionLimitToADeviceExceeded = 0x0A,
    ConnectionAlreadyExists = 0x0B,
    CommandDisallowed = 0x0C,
    ConnectionRejectedDueToLimitedResources = 0x0D,
    ConnectionRejectedDueToSecurityReasons = 0x0E,
    ConnectionRejectedDueToUnacceptableBdAddr = 0x0F,
    ConnectionAcceptTimeoutExceeded = 0x10,
    UnsupportedFeatureOrParameterValue = 0x11,
    InvalidCommandParameters = 0x12,
    RemoteUserTerminatedConnection = 0x13,
    RemoteDeviceTerminatedConnectionDueToLowResources = 0x14,
    RemoteDeviceTerminatedConnectionDueToPowerOff = 0x15,
    ConnectionTerminatedByLocalHost = 0x16,
    RepeatedAttempts = 0x17,
    PairingNotAllowed = 0x18,
    UnknownLmpPdu = 0x19,
    UnsupportedRemoteFeature = 0x1A,
    ScoOffsetRejected = 0x1B,
    ScoIntervalRejected = 0x1C,
    ScoAirModeRejected = 0x1D,
    InvalidLmpLlParameters = 0x1E,
    #[instructor(default)] // [Vol 4] Part E, Section 1.2
    UnspecifiedError = 0x1F,
    UnsupportedLmpLlParameterValue = 0x20,
    RoleChangeNotAllowed = 0x21,
    LmpLlResponseTimeout = 0x22,
    LmpLlErrorTransactionCollision = 0x23,
    LmpPduNotAllowed = 0x24,
    EncryptionModeNotAcceptable = 0x25,
    LinkKeyCannotBeChanged = 0x26,
    RequestedQosNotSupported = 0x27,
    InstantPassed = 0x28,
    PairingWithUnitKeyNotSupported = 0x29,
    DifferentTransactionCollision = 0x2A,
    QosUnacceptableParameter = 0x2C,
    QosRejected = 0x2D,
    ChannelClassificationNotSupported = 0x2E,
    InsufficientSecurity = 0x2F,
    ParameterOutOfMandatoryRange = 0x30,
    RoleSwitchPending = 0x32,
    ReservedSlotViolation = 0x34,
    RoleSwitchFailed = 0x35,
    ExtendedInquiryResponseTooLarge = 0x36,
    SecureSimplePairingNotSupportedByHost = 0x37,
    HostBusyPairing = 0x38,
    ConnectionRejectedDueToNoSuitableChannelFound = 0x39,
    ControllerBusy = 0x3A,
    UnacceptableConnectionParameters = 0x3B,
    AdvertisingTimeout = 0x3C,
    ConnectionTerminatedDueToMicFailure = 0x3D,
    ConnectionFailedToBeEstablished = 0x3E,
    CoarseClockAdjustmentRejected = 0x40,
    Type0SubmapNotDefined = 0x41,
    UnknownAdvertisingIdentifier = 0x42,
    LimitReached = 0x43,
    OperationCancelledByHost = 0x44,
    PacketTooLong = 0x45
}

impl Status {
    /// Returns whether status is `Success`.
    #[inline(always)]
    #[must_use]
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Success)
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for Status {}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct)]
pub struct EventMask(u64);

impl EventMask {
    /// Returns an all-zero event mask that disables all maskable events.
    #[inline(always)]
    pub const fn none() -> Self {
        Self(0)
    }

    pub fn all() -> Self {
        enum_iterator::all::<EventCode>().fold(EventMask::none(), |mask, e| mask.with(e, true))
    }

    // Enables or disables the specified event.
    #[inline(always)]
    pub fn with(mut self, c: EventCode, enable: bool) -> Self {
        let mask = c.to_mask_bits();
        if enable {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
        self
    }
}

impl Default for EventMask {
    fn default() -> Self {
        Self::all()
    }
}

impl EventCode {
    // ([Vol 4] Part E, Section 7.3.1)
    pub fn to_mask_bits(self) -> u64 {
        match self {
            EventCode::InquiryComplete => 1u64 << 0,
            EventCode::InquiryResult => 1u64 << 1,
            EventCode::ConnectionComplete => 1u64 << 2,
            EventCode::ConnectionRequest => 1u64 << 3,
            EventCode::DisconnectionComplete => 1u64 << 4,
            EventCode::AuthenticationComplete => 1u64 << 5,
            EventCode::RemoteNameRequestComplete => 1u64 << 6,
            EventCode::EncryptionChange => 1u64 << 7,
            EventCode::ChangeConnectionLinkKeyComplete => 1u64 << 8,
            EventCode::LinkKeyTypeChanged => 1u64 << 9,
            EventCode::ReadRemoteSupportedFeaturesComplete => 1u64 << 10,
            EventCode::ReadRemoteVersionInformationComplete => 1u64 << 11,
            EventCode::QosSetupComplete => 1u64 << 12,
            EventCode::HardwareError => 1u64 << 15,
            EventCode::FlushOccurred => 1u64 << 16,
            EventCode::RoleChange => 1u64 << 17,
            EventCode::ModeChange => 1u64 << 19,
            EventCode::ReturnLinkKeys => 1u64 << 20,
            EventCode::PinCodeRequest => 1u64 << 21,
            EventCode::LinkKeyRequest => 1u64 << 22,
            EventCode::LinkKeyNotification => 1u64 << 23,
            EventCode::LoopbackCommand => 1u64 << 24,
            EventCode::DataBufferOverflow => 1u64 << 25,
            EventCode::MaxSlotsChange => 1u64 << 26,
            EventCode::ReadClockOffsetComplete => 1u64 << 27,
            EventCode::ConnectionPacketTypeChanged => 1u64 << 28,
            EventCode::QosViolation => 1u64 << 29,
            EventCode::PageScanRepetitionModeChange => 1u64 << 31,
            EventCode::FlowSpecificationComplete => 1u64 << 32,
            EventCode::InquiryResultWithRssi => 1u64 << 33,
            EventCode::ReadRemoteExtendedFeaturesComplete => 1u64 << 34,
            EventCode::SynchronousConnectionComplete => 1u64 << 43,
            EventCode::SynchronousConnectionChanged => 1u64 << 44,
            EventCode::SniffSubrating => 1u64 << 45,
            EventCode::ExtendedInquiryResult => 1u64 << 46,
            EventCode::EncryptionKeyRefreshComplete => 1u64 << 47,
            EventCode::IoCapabilityRequest => 1u64 << 48,
            EventCode::IoCapabilityResponse => 1u64 << 49,
            EventCode::UserConfirmationRequest => 1u64 << 50,
            EventCode::UserPasskeyRequest => 1u64 << 51,
            EventCode::RemoteOobDataRequest => 1u64 << 52,
            EventCode::SimplePairingComplete => 1u64 << 53,
            EventCode::LinkSupervisionTimeoutChanged => 1u64 << 55,
            EventCode::EnhancedFlushComplete => 1u64 << 56,
            EventCode::UserPasskeyNotification => 1u64 << 58,
            EventCode::KeypressNotification => 1u64 << 59,
            EventCode::RemoteHostSupportedFeaturesNotification => 1u64 << 60,
            EventCode::LeMeta => 1u64 << 61,

            _ => 0
        }
    }
}
