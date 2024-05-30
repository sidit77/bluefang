use instructor::{BigEndian, Buffer, BufferMut, ByteSize, Error, Exstruct, Instruct};
use instructor::utils::Limit;
use crate::a2dp::sbc::SbcMediaCodecInformation;

use crate::avdtp::packets::{AudioCodec, MediaType, ServiceCategory, VideoCodec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    MediaTransport,
    MediaCodec(MediaCodecCapability),
    Generic(ServiceCategory, Vec<u8>),
}

impl Capability {
    pub fn is_basic(&self) -> bool {
        // ([AVDTP] Section 8.21.1).
        match self {
            Capability::Generic(ServiceCategory::DelayReporting, _) => false,
            _ => true
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaCodec {
    Audio(AudioCodec),
    Video(VideoCodec),
    Multimedia(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaCodecCapability {
    Sbc(SbcMediaCodecInformation),
    Generic(MediaCodec, Vec<u8>)
}

impl From<SbcMediaCodecInformation> for MediaCodecCapability {
    fn from(value: SbcMediaCodecInformation) -> Self {
        Self::Sbc(value)
    }
}

impl Exstruct<BigEndian> for Capability {
    #[inline]
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        let category: ServiceCategory = buffer.read_be()?;
        let length: u8 = buffer.read_be()?;
        let mut buffer = Limit::new(buffer, length as usize);
        let capability = match category {
            ServiceCategory::MediaTransport => Self::MediaTransport,
            ServiceCategory::MediaCodec => Self::MediaCodec(buffer.read_be()?),
            other => {
                let mut buf = vec![0; buffer.remaining()];
                buffer.try_copy_to_slice(&mut buf)?;
                Self::Generic(other, buf)
            }
        };
        buffer.finish()?;
        Ok(capability)
    }
}

impl Instruct<BigEndian> for Capability {

    #[inline]
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        let (cat, size) = match self {
            Capability::MediaTransport => (ServiceCategory::MediaTransport, 0),
            Capability::MediaCodec(codec) => (ServiceCategory::MediaCodec, codec.byte_size()),
            Capability::Generic(cat, info) => (*cat, info.byte_size())
        };
        buffer.write_be(&cat);
        buffer.write_be(&u8::try_from(size).expect("byte size is too large"));
        match self {
            Capability::MediaTransport => {}
            Capability::MediaCodec(codec) => buffer.write_be(codec),
            Capability::Generic(_, info) => buffer.extend_from_slice(info)
        }
    }
}

impl Exstruct<BigEndian> for MediaCodec {
    #[inline]
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        let mt: MediaTypeRaw = buffer.read_be()?;
        Ok(match mt.0 {
            MediaType::Audio => Self::Audio(buffer.read_be()?),
            MediaType::Video => Self::Audio(buffer.read_be()?),
            MediaType::Multimedia => Self::Multimedia(buffer.read_be()?),
        })
    }
}

impl Instruct<BigEndian> for MediaCodec {

    #[inline]
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        let (t, c) = match self {
            MediaCodec::Audio(codec) => (MediaType::Audio, *codec as u8),
            MediaCodec::Video(codec) => (MediaType::Video, *codec as u8),
            MediaCodec::Multimedia(codec) => (MediaType::Audio, *codec)
        };
        buffer.write_be(&(MediaTypeRaw(t), c));
    }
}

impl Exstruct<BigEndian> for MediaCodecCapability {
    #[inline]
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        let mc: MediaCodec = buffer.read_be()?;
        Ok(match mc {
            MediaCodec::Audio(AudioCodec::Sbc) => Self::Sbc(buffer.read_be()?),
            other => {
                let mut buf = vec![0; buffer.remaining()];
                buffer.try_copy_to_slice(&mut buf)?;
                Self::Generic(other, buf)
            }
        })
    }
}

impl Instruct<BigEndian> for MediaCodecCapability {

    #[inline]
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        match self {
            MediaCodecCapability::Sbc(info) => {
                buffer.write_be(&MediaCodec::Audio(AudioCodec::Sbc));
                buffer.write_be(info);
            }
            MediaCodecCapability::Generic(codec, info) => {
                buffer.write_be(codec);
                buffer.extend_from_slice(info);
            }
        }
    }
}

impl ByteSize for MediaCodecCapability {
    fn byte_size(&self) -> usize {
        2 + match self {
            MediaCodecCapability::Sbc(raw) => raw.byte_size(),
            MediaCodecCapability::Generic(_, raw) => raw.byte_size()
        }
    }
}

#[derive(Clone, Copy, Instruct, Exstruct)]
struct MediaTypeRaw (
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    MediaType
);

#[cfg(test)]
mod test {
    use bytes::{Buf, BytesMut};
    use instructor::{Buffer, BufferMut};
    use crate::a2dp::sbc::SbcMediaCodecInformation;

    use crate::avdtp::capabilities::{Capability, MediaCodecCapability};

    #[test]
    fn test_media_cap() {
        let packet_bytes: &[u8] = &[0x01, 0x00, 0x07, 0x06, 0x00, 0x00, 0xff, 0xff,  0x02, 0x35];
        let capabilites = vec![
            Capability::MediaTransport,
            Capability::MediaCodec(MediaCodecCapability::Sbc(SbcMediaCodecInformation::default()))
        ];
        let mut buf = BytesMut::new();
        buf.write(&capabilites);
        assert_eq!(buf.chunk(), packet_bytes);
        let read_caps: Vec<Capability> = buf.read().unwrap();
        assert_eq!(read_caps, capabilites);
    }

}