use instructor::{Exstruct, Instruct};
use thiserror::Error;
use tracing::error;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum ErrorCode {
    InvalidCommand = 0x00,
    InvalidParameter = 0x01,
    ParameterContentError = 0x02,
    InternalError = 0x03,
    NoError = 0x04,
    UidChanged = 0x05,
    InvalidDirection = 0x07,
    NotADirectory = 0x08,
    DoesNotExist = 0x09,
    InvalidScope = 0x0A,
    RangeOutOfBounds = 0x0B,
    FolderItemIsNotPlayable = 0x0C,
    MediaInUse = 0x0D,
    NowPlayingListFull = 0x0E,
    SearchNotSupported = 0x0F,
    SearchInProgress = 0x10,
    InvalidPlayerId = 0x11,
    PlayerNotBrowsable = 0x12,
    PlayerNotAddressed = 0x13,
    NoValidSearchResults = 0x14,
    NoAvailablePlayers = 0x15,
    AddressedPlayerChanged = 0x16
}

impl From<instructor::Error> for ErrorCode {
    #[track_caller]
    fn from(value: instructor::Error) -> Self {
        error!("Parsing error {} at {}", value, std::panic::Location::caller());
        Self::ParameterContentError
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Error)]
pub enum Error {
    #[error("The AVRCP session has been closed.")]
    SessionClosed,
    #[error("All 16 transaction ids are currently occupied.")]
    NoTransactionIdAvailable,
    #[error("The receiver does not implemented the command.")]
    NotImplemented,
    #[error("The receiver rejected the command (reason: {0:?}).")]
    Rejected(ErrorCode),
    #[error("The receiver is currently unable to perform this action due to being in a transient state.")]
    Busy,
    #[error("The returned data has an invalid format.")]
    InvalidReturnData
}


impl From<instructor::Error> for Error {
    #[track_caller]
    fn from(value: instructor::Error) -> Self {
        error!("Parsing error {} at {}", value, std::panic::Location::caller());
        Self::InvalidReturnData
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct NotImplemented;

impl From<instructor::Error> for NotImplemented {
    #[track_caller]
    fn from(value: instructor::Error) -> Self {
        error!("Parsing error {} at {}", value, std::panic::Location::caller());
        NotImplemented
    }
}

