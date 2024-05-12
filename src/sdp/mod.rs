use instructor::Exstruct;
use instructor::utils::Length;
use tokio::spawn;
use tracing::warn;
use crate::l2cap::channel::Channel;
use crate::l2cap::Server;

pub struct SdpServer;

impl Server for SdpServer {

    fn on_connection(&mut self, mut channel: Channel) {
        spawn(async move {
            if let Err(err) = channel.configure().await {
                warn!("Error configuring channel: {:?}", err);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        });
    }

}

#[derive(Debug, Exstruct)]
#[instructor(endian = "big")]
struct SdpHeader {
    pdu: PduId,
    transaction_id: u16,
    parameter_length: Length<u16, 0>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct)]
#[repr(u8)]
enum PduId {
    ErrorResponse = 0x01,
    ServiceSearchRequest = 0x02,
    ServiceSearchResponse = 0x03,
    ServiceAttributeRequest = 0x04,
    ServiceAttributeResponse = 0x05,
    ServiceSearchAttributeRequest = 0x06,
    ServiceSearchAttributeResponse = 0x07,
}



#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use instructor::Buffer;

    #[test]
    fn parse_packet() {
        let mut data = Bytes::from_static(&[
            0x06, 0x00, 0x00, 0x00, 0x0f,
            0x35, 0x03, 0x19, 0x12, 0x00,
            0x03, 0xf0, 0x35, 0x05, 0x0a,
            0x00, 0x00, 0xff, 0xff, 0x00]);

        let header: SdpHeader = data.read().unwrap();

    }

}