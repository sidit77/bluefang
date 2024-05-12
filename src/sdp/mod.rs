mod data_element;

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
    SearchRequest = 0x02,
    SearchResponse = 0x03,
    AttributeRequest = 0x04,
    AttributeResponse = 0x05,
    SearchAttributeRequest = 0x06,
    SearchAttributeResponse = 0x07,
}



fn handle_search_attribute_request() {

}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{Buf, Bytes};
    use instructor::Buffer;
    use crate::sdp::data_element::{DataElement, DataElementReader, Sequence, Uuid};

    #[test]
    fn parse_packet() {
        let mut data = Bytes::from_static(&[
            0x06, 0x00, 0x00, 0x00, 0x0f,
            0x35, 0x03, 0x19, 0x12, 0x00,
            0x03, 0xf0, 0x35, 0x05, 0x0a,
            0x00, 0x00, 0xff, 0xff, 0x00]);

        let header: SdpHeader = data.read().unwrap();
        println!("{:#?}", header);
        println!("ServiceSearchPatterns:");
        data.read_data_element::<Sequence<Uuid>>()
            .unwrap()
            .for_each(|attr| println!("   {}", attr.unwrap()));
        let max_attr_len: u16 = data.read_be().unwrap();
        println!("MaximumAttributeByteCount: {}", max_attr_len);
        println!("AttributeIDList:");
        data.read_data_element::<Sequence<u32>>()
            .unwrap()
            .for_each(|attr| println!("   {}", attr.unwrap()));
        let cont: u8 = data.read_be().unwrap();
        println!("cont: {:#?}", cont);
        data.advance(cont as usize);
        data.finish().unwrap();
    }

}