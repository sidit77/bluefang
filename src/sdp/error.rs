use instructor::{Exstruct, Instruct};

#[derive(Debug)]
pub enum Error {
    InvalidContinuationState,
    InvalidRequest,
    UnexpectedDataType,
    UnknownServiceRecordHandle(u32),
    MalformedPacketContent,
    UnexpectedPacketLength
}

impl From<Error> for SdpErrorCodes {
    fn from(value: Error) -> Self {
        match value {
            Error::InvalidContinuationState => Self::InvalidContinuationState,
            Error::InvalidRequest => Self::InvalidSdpVersion,
            Error::UnexpectedDataType => Self::InvalidRequestSyntax,
            Error::MalformedPacketContent => Self::InvalidSdpVersion,
            Error::UnexpectedPacketLength => Self::InvalidPduSize,
            Error::UnknownServiceRecordHandle(_) => Self::InvalidServiceRecordHandle
        }
    }
}

impl From<instructor::Error> for Error {
    fn from(value: instructor::Error) -> Self {
        use instructor::Error::*;
        match value {
            TooShort => Self::UnexpectedPacketLength,
            TooLong => Self::UnexpectedPacketLength,
            InvalidValue => Self::MalformedPacketContent,
            UnexpectedLength => Self::UnexpectedPacketLength
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u16)]
#[instructor(endian = "big")]
pub enum SdpErrorCodes {
    InvalidSdpVersion = 0x0001,
    InvalidServiceRecordHandle = 0x0002,
    InvalidRequestSyntax = 0x0003,
    InvalidPduSize = 0x0004,
    InvalidContinuationState = 0x0005,
    InsufficientResources = 0x0006
}
