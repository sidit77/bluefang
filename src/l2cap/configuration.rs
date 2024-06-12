use instructor::{Buffer, BufferMut, Error, Exstruct, Instruct, LittleEndian};
use tracing::{debug};
use crate::ensure;


trait ConfigurationOption: Default + Instruct<LittleEndian> + Exstruct<LittleEndian> + Into<ConfigurationParameter> {
    const TYPE: u8;
    const LENGTH: u8;
}

// ([Vol 3] Part A, Section 5.1)
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[instructor(endian = "little")]
pub struct Mtu(pub u16);

impl Mtu {
    const MINIMUM_ACL_U: Self = Self(48);
}

impl Default for Mtu {
    fn default() -> Self {
        Self::MINIMUM_ACL_U
    }
}

impl ConfigurationOption for Mtu {
    const TYPE: u8 = 0x01;
    const LENGTH: u8 = 2;
}

// ([Vol 3] Part A, Section 5.2)
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum FlushTimeout {
    NoRetransmission,
    Timeout(u16),
    #[default]
    Reliable
}

impl ConfigurationOption for FlushTimeout {
    const TYPE: u8 = 0x02;
    const LENGTH: u8 = 2;
}

impl Instruct<LittleEndian> for FlushTimeout {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        let value = match *self {
            FlushTimeout::NoRetransmission => 0x0001,
            FlushTimeout::Timeout(timeout) => {
                debug_assert!(timeout >= 0x0002 && timeout <= 0xFFFE);
                timeout
            },
            FlushTimeout::Reliable => 0xFFFF
        };
        buffer.write_le(value);
    }
}

impl Exstruct<LittleEndian> for FlushTimeout {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, instructor::Error> {
        match buffer.read_le::<u16>()? {
            0x0001 => Ok(FlushTimeout::NoRetransmission),
            0xFFFF => Ok(FlushTimeout::Reliable),
            timeout => {
                ensure!(timeout >= 0x0002 && timeout <= 0xFFFE, instructor::Error::InvalidValue);
                Ok(FlushTimeout::Timeout(timeout))
            }
        }
    }
}


#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum ServiceType {
    NoTraffic = 0x00,
    #[default]
    BestEffort = 0x01,
    Guaranteed = 0x02,
}

// ([Vol 3] Part A, Section 5.3)
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[instructor(endian = "little")]
pub struct QualityOfService {
    pub flags: u8,
    pub service_type: ServiceType,
    pub token_rate: u32,
    pub token_bucket_size: u32,
    pub peak_bandwidth: u32,
    pub latency: u32,
    pub delay_variation: u32
}

impl Default for QualityOfService {
    fn default() -> Self {
        Self {
            flags: 0,
            service_type: ServiceType::default(),
            token_rate: u32::MIN,
            token_bucket_size: u32::MIN,
            peak_bandwidth: u32::MIN,
            latency: u32::MAX,
            delay_variation: u32::MAX,
        }
    }
}

impl ConfigurationOption for QualityOfService {
    const TYPE: u8 = 0x03;
    const LENGTH: u8 = 22;
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum Mode {
    #[default]
    Basic = 0x00,
    Retransmission = 0x01,
    FlowControl = 0x02,
    EnhancedRetransmission = 0x03,
    Streaming = 0x04
}

// ([Vol 3] Part A, Section 5.4)
// The Basic L2CAP mode is the default. If Basic L2CAP mode is requested then all other parameters shall be ignored.
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[instructor(endian = "little")]
pub struct RetransmissionAndFlowControl {
    pub mode: Mode,
    pub tx_window_size: u8,
    pub max_transmit: u8,
    pub retransmission_timeout: u16,
    pub monitor_timeout: u16,
    pub mps: u16
}

impl ConfigurationOption for RetransmissionAndFlowControl {
    const TYPE: u8 = 0x04;
    const LENGTH: u8 = 9;
}

// ([Vol 3] Part A, Section 5.5)
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum Fcs {
    NoFcs = 0x00,
    #[default]
    Fcs16 = 0x01,
}

impl ConfigurationOption for Fcs {
    const TYPE: u8 = 0x05;
    const LENGTH: u8 = 1;
}

// ([Vol 3] Part A, Section 5.6)
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[instructor(endian = "little")]
pub struct ExtendedFlowSpecification {
    pub identifier: u8,
    pub service_type: ServiceType,
    pub max_sdu_size: u16,
    pub sdu_inter_time: u32,
    pub access_latency: u32,
    pub flush_timeout: u32
}

impl Default for ExtendedFlowSpecification {
    fn default() -> Self {
        Self {
            identifier: 0x01,
            service_type: ServiceType::BestEffort,
            max_sdu_size: u16::MAX,
            sdu_inter_time: u32::MAX,
            access_latency: u32::MAX,
            flush_timeout: u32::MAX
        }
    }
}

impl ConfigurationOption for ExtendedFlowSpecification {
    const TYPE: u8 = 0x06;
    const LENGTH: u8 = 16;
}

// ([Vol 3] Part A, Section 5.7)
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum ExtendedWindowSize {
    #[default]
    StreamingMode,
    EnhancedRetransmissionMode(u16)
}

impl Instruct<LittleEndian> for ExtendedWindowSize {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        match *self {
            ExtendedWindowSize::StreamingMode => buffer.write_le(0x0000u16),
            ExtendedWindowSize::EnhancedRetransmissionMode(size) => {
                debug_assert!(size >= 0x0001 && size <= 0x3FFF);
                buffer.write_le(size)
            }
        }
    }
}

impl Exstruct<LittleEndian> for ExtendedWindowSize {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        match buffer.read_le::<u16>()? {
            0x0000 => Ok(ExtendedWindowSize::StreamingMode),
            size => {
                ensure!(size >= 0x0001 && size <= 0x3FFF, Error::InvalidValue);
                Ok(ExtendedWindowSize::EnhancedRetransmissionMode(size))
            }
        }
    }
}

impl ConfigurationOption for ExtendedWindowSize {
    const TYPE: u8 = 0x07;
    const LENGTH: u8 = 2;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ConfigurationParameter {
    Mtu(Mtu),
    FlushTimeout(FlushTimeout),
    QualityOfService(QualityOfService),
    RetransmissionAndFlowControl(RetransmissionAndFlowControl),
    Fcs(Fcs),
    ExtendedFlowSpecification(ExtendedFlowSpecification),
    ExtendedWindowSize(ExtendedWindowSize),
    Unknown(u8)
}

impl Instruct<LittleEndian> for ConfigurationParameter {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        match self {
            ConfigurationParameter::Mtu(value) => {
                buffer.write_le_ref(&Mtu::TYPE);
                buffer.write_le_ref(&Mtu::LENGTH);
                buffer.write_le_ref(value);
            },
            ConfigurationParameter::FlushTimeout(value) => {
                buffer.write_le_ref(&FlushTimeout::TYPE);
                buffer.write_le_ref(&FlushTimeout::LENGTH);
                buffer.write_le_ref(value);
            },
            ConfigurationParameter::QualityOfService(value) => {
                buffer.write_le_ref(&QualityOfService::TYPE);
                buffer.write_le_ref(&QualityOfService::LENGTH);
                buffer.write_le_ref(value);
            },
            ConfigurationParameter::RetransmissionAndFlowControl(value) => {
                buffer.write_le_ref(&RetransmissionAndFlowControl::TYPE);
                buffer.write_le_ref(&RetransmissionAndFlowControl::LENGTH);
                buffer.write_le_ref(value);
            },
            ConfigurationParameter::Fcs(value) => {
                buffer.write_le_ref(&Fcs::TYPE);
                buffer.write_le_ref(&Fcs::LENGTH);
                buffer.write_le_ref(value);
            },
            ConfigurationParameter::ExtendedFlowSpecification(value) => {
                buffer.write_le_ref(&ExtendedFlowSpecification::TYPE);
                buffer.write_le_ref(&ExtendedFlowSpecification::LENGTH);
                buffer.write_le_ref(value);
            },
            ConfigurationParameter::ExtendedWindowSize(value) => {
                buffer.write_le_ref(&ExtendedWindowSize::TYPE);
                buffer.write_le_ref(&ExtendedWindowSize::LENGTH);
                buffer.write_le_ref(value);
            }
            ConfigurationParameter::Unknown(_) => {}
        }
    }
}

impl Exstruct<LittleEndian> for ConfigurationParameter {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        let ty: u8 = buffer.read_le()?;
        let len: u8 = buffer.read_le()?;
        match ty {
            Mtu::TYPE => {
                ensure!(len == Mtu::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::Mtu(buffer.read_le()?))
            },
            FlushTimeout::TYPE => {
                ensure!(len == FlushTimeout::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::FlushTimeout(buffer.read_le()?))
            },
            QualityOfService::TYPE => {
                ensure!(len == QualityOfService::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::QualityOfService(buffer.read_le()?))
            },
            RetransmissionAndFlowControl::TYPE => {
                ensure!(len == RetransmissionAndFlowControl::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::RetransmissionAndFlowControl(buffer.read_le()?))
            },
            Fcs::TYPE => {
                ensure!(len == Fcs::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::Fcs(buffer.read_le()?))
            },
            ExtendedFlowSpecification::TYPE => {
                ensure!(len == ExtendedFlowSpecification::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::ExtendedFlowSpecification(buffer.read_le()?))
            },
            ExtendedWindowSize::TYPE => {
                ensure!(len == ExtendedWindowSize::LENGTH, Error::InvalidValue);
                Ok(ConfigurationParameter::ExtendedWindowSize(buffer.read_le()?))
            },
            0x80..=0xFF => {
                debug!("Unsupported option: {:02X}", ty);
                buffer.skip(len as usize)?;
                Ok(ConfigurationParameter::Unknown(ty))
            },
            _ => Err(Error::InvalidValue)
        }
    }
}

impl From<Mtu> for ConfigurationParameter {
    fn from(value: Mtu) -> Self {
        ConfigurationParameter::Mtu(value)
    }
}

impl From<FlushTimeout> for ConfigurationParameter {
    fn from(value: FlushTimeout) -> Self {
        ConfigurationParameter::FlushTimeout(value)
    }
}

impl From<QualityOfService> for ConfigurationParameter {
    fn from(value: QualityOfService) -> Self {
        ConfigurationParameter::QualityOfService(value)
    }
}

impl From<RetransmissionAndFlowControl> for ConfigurationParameter {
    fn from(value: RetransmissionAndFlowControl) -> Self {
        ConfigurationParameter::RetransmissionAndFlowControl(value)
    }
}

impl From<Fcs> for ConfigurationParameter {
    fn from(value: Fcs) -> Self {
        ConfigurationParameter::Fcs(value)
    }
}

impl From<ExtendedFlowSpecification> for ConfigurationParameter {
    fn from(value: ExtendedFlowSpecification) -> Self {
        ConfigurationParameter::ExtendedFlowSpecification(value)
    }
}

impl From<ExtendedWindowSize> for ConfigurationParameter {
    fn from(value: ExtendedWindowSize) -> Self {
        ConfigurationParameter::ExtendedWindowSize(value)
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use instructor::BufferMut;
    use crate::l2cap::configuration::{ConfigurationOption, ExtendedFlowSpecification, ExtendedWindowSize, Fcs, FlushTimeout, Mtu, QualityOfService, RetransmissionAndFlowControl};

    #[test]
    fn check_sizes() {
        fn check_size<T: ConfigurationOption>() {
            let mut buffer = BytesMut::new();
            buffer.write_le(T::default());
            assert_eq!(buffer.len(), T::LENGTH as usize);
        }

        check_size::<Mtu>();
        check_size::<FlushTimeout>();
        check_size::<QualityOfService>();
        check_size::<RetransmissionAndFlowControl>();
        check_size::<Fcs>();
        check_size::<ExtendedFlowSpecification>();
        check_size::<ExtendedWindowSize>();
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