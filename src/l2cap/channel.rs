use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;
use bytes::{Bytes, BytesMut};
use instructor::{Buffer, BufferMut, DoubleEndedBufferMut};
use instructor::utils::Length;
use tokio::sync::mpsc::{UnboundedReceiver as MpscReceiver};
use tokio::time::sleep;
use tracing::trace;
use crate::ensure;
use crate::hci::{AclSender, Error};
use crate::l2cap::{ChannelEvent, CID_ID_SIGNALING, ConfigureResult, L2capHeader};
use crate::l2cap::signaling::{SignalingCodes, SignalingHeader};

const DEFAULT_MTU: u16 = 1691;

pub struct Channel {
    pub connection_handle: u16,
    pub remote_cid: u16,
    pub local_cid: u16,
    pub receiver: MpscReceiver<ChannelEvent>,
    pub sender: AclSender,
    pub next_signaling_id: Arc<AtomicU8>,
    pub local_mtu: u16,
    pub remote_mtu: u16,
}

impl Channel {

    fn send_configure_signal(&self, code: SignalingCodes, id: u8, mut options: BytesMut) -> Result<(), Error> {
        options.write_front(&SignalingHeader {
            code,
            id,
            length: Length::new(options.len())?,
        });
        options.write_front(&L2capHeader {
            len: Length::new(options.len())?,
            cid: CID_ID_SIGNALING,
        });
        self.sender.send(self.connection_handle, options.freeze())?;
        Ok(())
    }

    pub async fn configure(&mut self) -> Result<(), Error> {
        sleep(Duration::from_millis(400)).await;
        let mut options = BytesMut::new();
        options.write_le(&self.remote_cid);
        options.write_le(&0x0000u16);
        options.write_le(&0x01u8);
        options.write_le(&0x02u8);
        options.write_le(&DEFAULT_MTU);
        let waiting_id = self.next_signaling_id.fetch_add(1, Ordering::Relaxed);
        self.send_configure_signal(SignalingCodes::ConfigureRequest, waiting_id, options)?;

        while self.local_mtu == 0 || self.remote_mtu == 0 {
            match self.receiver.recv().await.ok_or(Error::Generic("Configure failed"))? {
                ChannelEvent::DataReceived(_) => trace!("Received data while still configuring"),
                ChannelEvent::ConfigurationRequest(id, mut options) => {
                    let mut resp = BytesMut::new();
                    resp.write_le(&self.remote_cid);
                    resp.write_le(&0x0000u16);
                    resp.write_le(&ConfigureResult::Success);
                    if options.is_empty() {
                        self.send_configure_signal(SignalingCodes::ConfigureResponse, id, resp)?;
                    } else {
                        ensure!(options.read_le::<u8>()? == 0x01, "Expected MTU");
                        ensure!(options.read_le::<u8>()? == 0x02, "Expected length 2");
                        self.remote_mtu = options.read_le()?;
                        resp.write_le(&0x01u8);
                        resp.write_le(&0x02u8);
                        resp.write_le(&self.remote_mtu);
                        self.send_configure_signal(SignalingCodes::ConfigureResponse, id, resp)?;
                    }

                }
                ChannelEvent::ConfigurationResponse(id, result, mut options) => {
                    ensure!(result == ConfigureResult::Success, "Configuration failed");
                    ensure!(id == waiting_id, "unknown configure id");
                    if !options.is_empty() {
                        ensure!(options.read_le::<u8>()? == 0x01, "Expected MTU");
                        ensure!(options.read_le::<u8>()? == 0x02, "Expected length 2");
                        self.local_mtu = options.read_le()?;
                    } else {
                        self.local_mtu = DEFAULT_MTU;
                    }
                }
            }
        }

        trace!("Channel configured: local_mtu={:04X} remote_mtu={:04X}", self.local_mtu, self.remote_mtu);
        Ok(())
    }

}


/*
let mut return_data = BytesMut::new();
        let mut result = ConfigureResult::Success;
        while !data.is_empty() {
            // ([Vol 3] Part A, Section 5).
            let option_type: u8 = data.read_le()?;
            let option_len: u8 = data.read_le()?;
            match option_type {
                // MTU - ([Vol 3] Part A, Section 5.1)
                0x01 => {
                    ensure!(option_len == 2, Error::BadPacket(instructor::Error::InvalidValue));
                    let mtu: u16 = data.read_le()?;
                    debug!("            MTU: {:04X}", mtu);

                    return_data.write_le(&option_type);
                    return_data.write_le(&option_len);
                    return_data.write_le(&mtu);
                },
                // Flush timeout - ([Vol 3] Part A, Section 5.2)
                0x02 => {
                    ensure!(option_len == 2, Error::BadPacket(instructor::Error::InvalidValue));
                    let flush_timeout: u16 = data.read_le()?;
                    debug!("            Flush timeout: {:04X}", flush_timeout);
                },
                // QoS - ([Vol 3] Part A, Section 5.3)
                0x03 => {
                    ensure!(option_len == 22, Error::BadPacket(instructor::Error::InvalidValue));
                    let flags: u8 = data.read_le()?;
                    let service_type: u8 = data.read_le()?;
                    let token_rate: u32 = data.read_le()?;
                    let token_bucket_size: u32 = data.read_le()?;
                    let peak_bandwidth: u32 = data.read_le()?;
                    let latency: u32 = data.read_le()?;
                    let delay_variation: u32 = data.read_le()?;
                    debug!("            QoS: flags={:02X} service_type={:02X} token_rate={:08X} token_bucket_size={:08X} peak_bandwidth={:08X} latency={:08X} delay_variation={:08X}",
                        flags, service_type, token_rate, token_bucket_size, peak_bandwidth, latency, delay_variation);
                },
                // Retransmission and flow control - ([Vol 3] Part A, Section 5.4)
                0x04 => {
                    ensure!(option_len == 9, Error::BadPacket(instructor::Error::InvalidValue));
                    let mode: u8 = data.read_le()?;
                    let tx_window_size: u8 = data.read_le()?;
                    let max_transmit: u8 = data.read_le()?;
                    let retransmission_timeout: u16 = data.read_le()?;
                    let monitor_timeout: u16 = data.read_le()?;
                    let mps: u16 = data.read_le()?;
                    debug!("            Retransmission and flow control: mode={:02X} tx_window_size={:02X} max_transmit={:02X} retransmission_timeout={:04X} monitor_timeout={:04X} mps={:04X}",
                        mode, tx_window_size, max_transmit, retransmission_timeout, monitor_timeout, mps);
                },
                // FCS - ([Vol 3] Part A, Section 5.5)
                0x05 => {
                    ensure!(option_len == 1, Error::BadPacket(instructor::Error::InvalidValue));
                    let fcs: u8 = data.read_le()?;
                    debug!("            FCS: {:02X}", fcs);
                },
                // Extended flow specification - ([Vol 3] Part A, Section 5.6)
                0x06 => {
                    ensure!(option_len == 16, Error::BadPacket(instructor::Error::InvalidValue));
                    let identifier: u8 = data.read_le()?;
                    let service_type: u8 = data.read_le()?;
                    let max_sdu_size: u16 = data.read_le()?;
                    let sdu_inter_time: u32 = data.read_le()?;
                    let access_latency: u32 = data.read_le()?;
                    let flush_timeout: u32 = data.read_le()?;
                    debug!("            Extended flow specification: identifier={:02X} service_type={:02X} max_sdu_size={:04X} sdu_inter_time={:08X} access_latency={:08X} flush_timeout={:08X}",
                        identifier, service_type, max_sdu_size, sdu_inter_time, access_latency, flush_timeout);
                }
                // Extended window size - ([Vol 3] Part A, Section 5.7)
                0x07 => {
                    ensure!(option_len == 2, Error::BadPacket(instructor::Error::InvalidValue));
                    let tx_window_size: u16 = data.read_le()?;
                    debug!("            Extended window size: {:04X}", tx_window_size);
                },
                0x80..=0xFF => {
                    warn!("            Unsupported option: type={:02X}", option_type);
                    data.advance(option_len as usize);
                },
                _ => {
                    result = ConfigureResult::UnknownOptions;
                    return_data.clear();
                    return_data.write_le(&option_type);
                    break;
                },
            }
        }
 */