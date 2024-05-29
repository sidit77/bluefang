use bitflags::bitflags;
use instructor::{ByteSize, Exstruct, Instruct};

// ([A2DP] Section 4.3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[instructor(endian = "big")]
pub struct SbcMediaCodecInformation {
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    pub sampling_frequencies: SamplingFrequencies,
    #[instructor(bits(0..4))]
    pub channel_modes: ChannelModes,
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    pub block_lengths: BlockLengths,
    #[instructor(bits(2..4))]
    pub subbands: Subbands,
    #[instructor(bits(0..2))]
    pub allocation_methods: AllocationMethods,
    pub minimum_bitpool: u8,
    pub maximum_bitpool: u8,
}

impl Default for SbcMediaCodecInformation {
    fn default() -> Self {
        SbcMediaCodecInformation {
            sampling_frequencies: SamplingFrequencies::all(),
            channel_modes: ChannelModes::all(),
            block_lengths: BlockLengths::all(),
            subbands: Subbands::all(),
            allocation_methods: AllocationMethods::all(),
            minimum_bitpool: 2,
            maximum_bitpool: 53,
        }
    }
}

// ([A2DP] Section 4.3.2.1).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
    #[instructor(bitflags)]
    pub struct SamplingFrequencies: u8 {
        const FREQ_16000 = 0b1000;
        const FREQ_32000 = 0b0100;
        const FREQ_44100 = 0b0010;
        const FREQ_48000 = 0b0001;
    }
}

impl SamplingFrequencies {
    pub fn as_value(self) -> Option<u32> {
        match self {
            SamplingFrequencies::FREQ_16000 => Some(16000),
            SamplingFrequencies::FREQ_32000 => Some(32000),
            SamplingFrequencies::FREQ_44100 => Some(44100),
            SamplingFrequencies::FREQ_48000 => Some(48000),
            _ => None,
        }
    }
}

// ([A2DP] Section 4.3.2.2).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
    #[instructor(bitflags)]
    pub struct ChannelModes: u8 {
        const MONO = 0b1000;
        const DUAL_CHANNEL = 0b0100;
        const STEREO = 0b0010;
        const JOINT_STEREO = 0b0001;
    }
}

// ([A2DP] Section 4.3.2.3).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
    #[instructor(bitflags)]
    pub struct BlockLengths: u8 {
        const FOUR = 0b1000;
        const EIGHT = 0b0100;
        const TWELVE = 0b0010;
        const SIXTEEN = 0b0001;
    }
}

impl BlockLengths {
    pub fn as_value(self) -> Option<u32> {
        match self {
            BlockLengths::FOUR => Some(4),
            BlockLengths::EIGHT => Some(8),
            BlockLengths::TWELVE => Some(12),
            BlockLengths::SIXTEEN => Some(16),
            _ => None,
        }
    }
}

// ([A2DP] Section 4.3.2.4).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
    #[instructor(bitflags)]
    pub struct Subbands: u8 {
        const FOUR = 0b10;
        const EIGHT = 0b01;
    }
}

impl Subbands {
    pub fn as_value(self) -> Option<u32> {
        match self {
            Subbands::FOUR => Some(4),
            Subbands::EIGHT => Some(8),
            _ => None,
        }
    }
}

// ([A2DP] Section 4.3.2.5).
bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
    #[instructor(bitflags)]
    pub struct AllocationMethods: u8 {
        const SNR = 0b10;
        const LOUDNESS = 0b01;
    }
}

//TODO Replace with derive
impl ByteSize for SbcMediaCodecInformation {
    fn byte_size(&self) -> usize {
        4
    }
}


#[cfg(test)]
mod test {
    use bytes::Bytes;
    use instructor::Buffer;
    use crate::a2dp::sbc::SbcMediaCodecInformation;

    #[test]
    fn test_sbc_codec_information() {
        let testdata: &[u8] = &[0xff, 0xff, 0x02, 0x35];
        let mut data = Bytes::from_static(testdata);
        let codec: SbcMediaCodecInformation = data.read().unwrap();
        println!("{:#?}", codec);
    }
}