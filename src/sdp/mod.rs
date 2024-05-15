mod data_element;
mod error;
mod service;

use std::collections::BTreeMap;
use std::mem::size_of;
use std::sync::Arc;
use bytes::{BufMut, BytesMut};
use instructor::{Buffer, BufferMut, Exstruct, Instruct};
use instructor::utils::Length;
use tokio::spawn;
use tracing::{error, trace, warn};
use crate::{ensure, hci};
use crate::l2cap::channel::Channel;
use crate::l2cap::Server;
use crate::sdp::error::Error;
use crate::sdp::service::{Service, ServiceAttribute};

pub use data_element::{DataElement, Uuid};

/*
#[derive(Default, Clone)]
pub struct SdpServerBuilder {
    records: BTreeMap<Uuid, BTreeMap<u16, DataElement>>
}

impl SdpServerBuilder {
    pub fn add_records(mut self, service: impl Into<Uuid>, records: impl IntoIterator<Item=(u16, DataElement)>) -> Self {
        self
            .records
            .entry(service.into())
            .or_default()
            .extend(records.into_iter().map(|(id, value)| (id, value.into())));
        self
    }

    pub fn build(self) -> SdpServer {
        SdpServer {
            records: Arc::new(self.records)
        }
    }
}
*/



#[derive(Clone)]
pub struct SdpServer {
    records: Arc<BTreeMap<u32, Service>>
}

const SDP_SERVICE_RECORD_HANDLE_ATTRIBUTE_ID: u16 = 0x0000;
const SDP_SERVICE_CLASS_ID_LIST_ATTRIBUTE_ID: u16 = 0x0001;
const SDP_PROTOCOL_DESCRIPTOR_LIST_ATTRIBUTE_ID: u16 = 0x0004;
const SDP_BROWSE_GROUP_LIST_ATTRIBUTE_ID: u16 = 0x0005;
const SDP_BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ATTRIBUTE_ID: u16 = 0x0009;


const SDP_PUBLIC_BROWSE_ROOT: Uuid = Uuid::from_u16(0x1002);
const BT_AUDIO_SINK_SERVICE: Uuid = Uuid::from_u16(0x110b);
const BT_L2CAP_PROTOCOL_ID: Uuid = Uuid::from_u16(0x0100);
const BT_AVDTP_PROTOCOL_ID: Uuid = Uuid::from_u16(0x0019);
const BT_ADVANCED_AUDIO_DISTRIBUTION_SERVICE: Uuid = Uuid::from_u16(0x110d);

const AVDTP_PSM: u16 = 0x0019;

impl Default for SdpServer {
    fn default() -> Self {
        //SdpServerBuilder::default()
        //    .add_records(PNP_INFORMATION, [])
        //    .build()
        let service_record_handle = 0x00010001;
        let version = 1u16 << 8 | 3u16;
        SdpServer {
            records: Arc::new(BTreeMap::from_iter([
                (service_record_handle, Service::from_iter([
                    ServiceAttribute::new(SDP_SERVICE_RECORD_HANDLE_ATTRIBUTE_ID, service_record_handle),
                    ServiceAttribute::new(SDP_BROWSE_GROUP_LIST_ATTRIBUTE_ID, DataElement::from_iter([
                        SDP_PUBLIC_BROWSE_ROOT,
                    ])),
                    ServiceAttribute::new(SDP_SERVICE_CLASS_ID_LIST_ATTRIBUTE_ID, DataElement::from_iter([
                        BT_AUDIO_SINK_SERVICE,
                    ])),
                    ServiceAttribute::new(SDP_PROTOCOL_DESCRIPTOR_LIST_ATTRIBUTE_ID, DataElement::from_iter([
                        (BT_L2CAP_PROTOCOL_ID, AVDTP_PSM),
                        (BT_AVDTP_PROTOCOL_ID, version)
                    ])),
                    ServiceAttribute::new(SDP_BLUETOOTH_PROFILE_DESCRIPTOR_LIST_ATTRIBUTE_ID, DataElement::from_iter([
                        (BT_ADVANCED_AUDIO_DISTRIBUTION_SERVICE, version)
                    ])),
                ]))
            ])),
        }
    }
}

impl Server for SdpServer {

    fn on_connection(&mut self, mut channel: Channel) {
        let server = self.clone();
        spawn(async move {
            if let Err(err) = channel.configure().await {
                warn!("Error configuring channel: {:?}", err);
                return;
            }
            server.handle_connection(channel).await.unwrap_or_else(|err| {
                warn!("Error handling connection: {:?}", err);
            });
            trace!("SDP connection closed");
        });
    }

}

fn catch_error<F, E, R>(f: F) -> Result<R, E>
    where F: FnOnce() -> Result<R, E>
{
    f()
}

const CONTINUATION_STATE: [u8; 4] = *b"cont";
impl SdpServer {

    async fn handle_connection(self, mut channel: Channel) -> Result<(), hci::Error> {
        let mut buffer = BytesMut::new();
        while let Some(mut request) = channel.read().await {
            //TODO handle errors more gracefully
            let SdpHeader {pdu, transaction_id, ..} = request.read()?;
            let reply = catch_error(|| match pdu {
                // ([Vol 3] Part B, Section 4.7.1).
                PduId::SearchAttributeRequest => {
                    let service_search_patterns: DataElement = request.read()?;
                    let max_attr_len: usize = request.read_be::<u16>()? as usize;
                    let attributes: DataElement = request.read()?;
                    let cont: u8 = request.read_be()?;
                    match cont {
                        0 => {
                            buffer.clear();
                            buffer.write(&self.collect_records(service_search_patterns, attributes)?);
                        },
                        4 => {
                            ensure!(request.read_be::<[u8; 4]>()? == CONTINUATION_STATE, Error::InvalidContinuationState);
                            ensure!(!buffer.is_empty(), Error::InvalidContinuationState);
                        },
                        _ => return Err(Error::InvalidContinuationState)
                    }
                    request.finish()?;
                    let to_send = buffer.split_to(max_attr_len.min(buffer.len()));
                    let end = buffer.is_empty();
                    let mut packet = BytesMut::new();
                    packet.write(&SdpHeader {
                        pdu: PduId::SearchAttributeResponse,
                        transaction_id,
                        parameter_length: Length::new(
                            size_of::<u16>() +
                                to_send.len() +
                                size_of::<u8>() +
                                !end
                                    .then_some(CONTINUATION_STATE.len())
                                    .unwrap_or_default())?,
                    });
                    packet.write_be(&(to_send.len() as u16));
                    packet.put(to_send);
                    match end {
                        true => packet.write_be(&0u8),
                        false => {
                            packet.write_be(&4u8);
                            packet.write_be(&CONTINUATION_STATE);
                        }
                    }
                    Ok(packet.freeze())
                }
                _ => {
                    warn!("Unsupported PDU: {:?}", pdu);
                    Err(Error::InvalidRequest)
                }
            }).map(Some).unwrap_or_else(|err| {
                error!("Error handling request: {:?}", err);
                None
            });
            if let Some(reply) = reply {
                channel.write(reply)?;
            }
        }
        Ok(())
    }

    fn collecting_matching_records<'a: 'b, 'b>(&'a self, service_search_patterns: &'b [Uuid]) -> impl Iterator<Item=&'a Service> + 'b {
        self
            .records
            .values()
            .filter(move |&service| service_search_patterns
                .iter()
                .any(|&uuid| service.contains(uuid)))
    }

    fn collect_records(&self, service_search_patterns: DataElement, attribute_list: DataElement) -> Result<DataElement, Error> {
        let attributes = attribute_list
            .as_sequence()?
            .iter()
            .map(|element| match element {
                DataElement::U16(id) => Ok(*id..=*id),
                DataElement::U32(range) => {
                    let start = (*range >> 16) as u16;
                    let end = (*range & 0xFFFF) as u16;
                    Ok(start..=end)
                }
                _ => Err(Error::UnexpectedDataType)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let service_search_patterns = service_search_patterns
            .as_sequence()?
            .iter()
            .map(|element| element.as_uuid())
            .collect::<Result<Vec<_>, _>>()?;

        let attribute_list = self
            .collecting_matching_records(&service_search_patterns)
            .map(|service| service
                .attributes(&attributes)
                .cloned()
                .flat_map(ServiceAttribute::into_iter)
                .collect::<DataElement>())
            .filter(|element| !element.is_empty())
            .collect::<DataElement>();

        Ok(attribute_list)
    }

}

#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "big")]
struct SdpHeader {
    pdu: PduId,
    transaction_id: u16,
    parameter_length: Length<u16, 0>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
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
    use crate::sdp::data_element::{DataElement};

    #[test]
    fn parse_packet() {
        //let mut data = Bytes::from_static(&[
        //    0x06, 0x00, 0x00, 0x00, 0x0f,
        //    0x35, 0x03, 0x19, 0x12, 0x00,
        //    0x03, 0xf0, 0x35, 0x05, 0x0a,
        //    0x00, 0x00, 0xff, 0xff, 0x00]);

        let mut data = Bytes::from_static(&[
            0x06, 0x00, 0x00, 0x00, 0x0f,
            0x35, 0x03, 0x19, 0x01, 0x00,
            0x03, 0xf0, 0x35, 0x05, 0x0a,
            0x00, 0x00, 0xff, 0xff, 0x00]);

        let header: SdpHeader = data.read().unwrap();
        println!("{:#?}", header);
        let service_search_patterns: DataElement = data.read().unwrap();
        let _max_attr_len: u16 = data.read_be().unwrap();
        let attributes: DataElement = data.read().unwrap();
        let cont: u8 = data.read_be().unwrap();
        data.advance(cont as usize);
        data.finish().unwrap();
        let sdp = SdpServer::default();
        let records = sdp.collect_records(service_search_patterns, attributes).unwrap();
        println!("{:?}", records);
        let mut buffer = BytesMut::new();
        buffer.write(&records);
        println!("{:x?}", buffer.chunk());
        let expected = &[
            0x35, 0x3c, 0x35, 0x3a, 0x09, 0x00, 0x00, 0x0a, 0x00, 0x01, 0x00, 0x01, 0x09, 0x00, 0x01, 0x35,
            0x03, 0x19, 0x11, 0x0b, 0x09, 0x00, 0x04, 0x35, 0x10, 0x35, 0x06, 0x19, 0x01, 0x00, 0x09, 0x00,
            0x19, 0x35, 0x06, 0x19, 0x00, 0x19, 0x09, 0x01, 0x03, 0x09, 0x00, 0x05, 0x35, 0x03, 0x19, 0x10,
            0x02, 0x09, 0x00, 0x09, 0x35, 0x08, 0x35, 0x06, 0x19, 0x11, 0x0d, 0x09, 0x01, 0x03
        ];
        assert_eq!(buffer.chunk(), expected);
    }

}