use instructor::{ByteSize, Exstruct, Instruct};
use instructor::utils::u24;

// ([A2DP] Section 4.3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[instructor(endian = "big")]
pub struct SbcMediaCodecInformationRaw {
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    pub sampling_frequency: u8,
    #[instructor(bits(0..4))]
    pub channel_mode: u8,
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    pub block_length: u8,
    #[instructor(bits(2..4))]
    pub subbands: u8,
    #[instructor(bits(0..2))]
    pub allocation_method: u8,
    pub minimum_bitpool: u8,
    pub maximum_bitpool: u8,
}

//TODO Replace with derive
impl ByteSize for SbcMediaCodecInformationRaw {
    fn byte_size(&self) -> usize {
        4
    }
}

// ([A2DP] Section 4.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[instructor(endian = "big")]
pub struct AacMediaCodecInformationRaw {
    #[instructor(bitfield(u8))]
    #[instructor(bits(1..8))]
    pub object_type: u8,
    #[instructor(bits(0..1))]
    pub drc: bool,
    #[instructor(bitfield(u16))]
    #[instructor(bits(4..16))]
    pub sampling_frequency: u16,
    #[instructor(bits(0..4))]
    pub channels: u8,
    #[instructor(bitfield(u24))]
    #[instructor(bits(23..24))]
    pub vbr: bool,
    #[instructor(bits(0..23))]
    pub bit_rate: u24,
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use instructor::Buffer;
    use crate::a2dp::{AacMediaCodecInformationRaw, SbcMediaCodecInformationRaw};

    #[test]
    fn test_sbc_codec_information() {
        let testdata: &[u8] = &[0xff, 0xff, 0x02, 0x35];
        let mut data = Bytes::from_static(testdata);
        let codec: SbcMediaCodecInformationRaw = data.read().unwrap();
        println!("{:#?}", codec);
    }

    #[test]
    fn test_aac_codec_information() {
        let testdata: &[u8] = &[0x80, 0x01, 0x8c, 0x84, 0x09, 0xb6];
        let mut data = Bytes::from_static(testdata);
        let codec: AacMediaCodecInformationRaw = data.read().unwrap();
        println!("{:#?}", codec);
        println!("{:06x}", codec.bit_rate);
    }

}