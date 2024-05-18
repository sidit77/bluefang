use instructor::{Error as InstructorError, Exstruct, Instruct};
use thiserror::Error;

// [AVDTP] Section 8.20.6.2.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct, Error)]
#[repr(u8)]
pub enum ErrorCode {
    /// Caused by commands: All messages
    #[error("Bad header format")]
    BadHeaderFormat = 0x01,


    /// Caused by commands: All messages
    #[error("The request packet length is not match the assumed length")]
    BadLength = 0x11,

    /// Caused by commands: All messages
    #[error("The requested command indicates an invalid ACP SEID (not addressable)")]
    BadAcpSeid = 0x12,

    /// Caused by commands: SetConfiguration
    #[error("The SEP is in use")]
    SepInUse = 0x13,

    /// Caused by commands: Reconfigure
    #[error("The SEP is not in use")]
    SepNotInUse = 0x14,

    /// Caused by commands: SetConfiguration, Reconfigure
    #[error("The value of Service Category in the request packet is not defined in AVDTP")]
    BadServCategory = 0x17,

    /// Caused by commands: All messages
    #[error("The requested command has an incorrect payload format")]
    BadPayloadFormat = 0x18,

    /// Caused by commands: All messages
    #[error("The requested command is not supported by the device")]
    NotSupportedCommand = 0x19,

    /// Caused by commands: Reconfigure
    #[error("The reconfigure command is an attempt to reconfigure a transport service capabilities of the SEP. \
              Reconfigure is only permitted for application service capabilities")]
    InvalidCapabilities = 0x1A,


    /// Caused by commands: SetConfiguration
    #[error("The requested Recovery Type is not defined in AVDTP")]
    BadRecoveryType = 0x22,

    /// Caused by commands: SetConfiguration
    #[error("The format of Media Transport Capability is not correct")]
    BadMediaTransportFormat = 0x23,

    /// Caused by commands: SetConfiguration
    #[error("The format of Recovery Service Capability is not correct")]
    BadRecoveryFormat = 0x25,

    /// Caused by commands: SetConfiguration
    #[error("Protection Service Capability is not correct")]
    BadRohcFormat = 0x26,

    /// Caused by commands: SetConfiguration
    #[error("The format of Content Protection is not correct")]
    BadContentProtectionFormat = 0x27,

    /// Caused by commands: SetConfiguration
    #[error("The format of Multiplexing Service Capability is not correct")]
    BadMultiplexingFormat = 0x28,

    /// Caused by commands: SetConfiguration
    #[error("Configuration not supported")]
    UnsupportedConfiguration = 0x29,

    /// Caused by commands: All messages
    #[error("The ACP state machine is in an invalid state in order to process the signal")]
    BadState = 0x31,
}

impl From<InstructorError> for ErrorCode {
    fn from(value: InstructorError) -> Self {
        match value {
            InstructorError::TooShort => ErrorCode::BadLength,
            InstructorError::TooLong => ErrorCode::BadLength,
            InstructorError::InvalidValue => ErrorCode::BadHeaderFormat,
            InstructorError::UnexpectedLength => ErrorCode::BadLength,
        }
    }
}