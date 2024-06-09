use instructor::{Exstruct, Instruct};

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
    fn from(_: instructor::Error) -> Self {
        Self::ParameterContentError
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AvcError {
    NotImplemented //Parsing(instructor::Error),
                   //Avrcp(ErrorCode),
}

impl From<instructor::Error> for AvcError {
    fn from(_value: instructor::Error) -> Self {
        //Self::Parsing(value)
        // idk
        Self::NotImplemented
    }
}

//impl From<ErrorCode> for AvcError {
//    fn from(value: ErrorCode) -> Self {
//        Self::Avrcp(value)
//    }
//}
