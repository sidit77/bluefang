use std::fmt::{Debug, Formatter};

use bitflags::bitflags;
use instructor::utils::u24;
use instructor::{BitBuffer, Buffer, BufferMut, Error, Exstruct, Instruct, LittleEndian};

use crate::utils::DebugFn;

/// Class of Device ([Assigned Numbers] Section 2.8).
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ClassOfDevice {
    pub service_classes: MajorServiceClasses,
    pub device_class: DeviceClass,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
enum MajorDeviceClassId {
    Miscellaneous = 0x00,
    Computer = 0x01,
    Phone = 0x02,
    LanAccessPoint = 0x03,
    AudioVideo = 0x04,
    Peripheral = 0x05,
    Imaging = 0x06,
    Wearable = 0x07,
    Toy = 0x08,
    Health = 0x09,
    #[instructor(default)]
    Uncategorized = 0x1F
}

impl Instruct<LittleEndian> for ClassOfDevice {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        let mut bitfield = BitBuffer::<u24>::empty();
        bitfield.set_range(13, 24);
        bitfield.write_be(self.service_classes);
        bitfield.set_range(8, 13);
        match self.device_class {
            DeviceClass::Miscellaneous => {
                bitfield.write_be(MajorDeviceClassId::Miscellaneous);
            }
            DeviceClass::Computer(minor) => {
                bitfield.write_be(MajorDeviceClassId::Computer);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Phone(minor) => {
                bitfield.write_be(MajorDeviceClassId::Phone);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::LanAccessPoint(minor) => {
                bitfield.write_be(MajorDeviceClassId::LanAccessPoint);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::AudioVideo(minor) => {
                bitfield.write_be(MajorDeviceClassId::AudioVideo);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Peripheral(minor) => {
                bitfield.write_be(MajorDeviceClassId::Peripheral);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Imaging(minor) => {
                bitfield.write_be(MajorDeviceClassId::Imaging);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Wearable(minor) => {
                bitfield.write_be(MajorDeviceClassId::Wearable);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Toy(minor) => {
                bitfield.write_be(MajorDeviceClassId::Toy);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Health(minor) => {
                bitfield.write_be(MajorDeviceClassId::Health);
                bitfield.set_range(2, 8);
                bitfield.write_be(minor);
            }
            DeviceClass::Uncategorized => {
                bitfield.write_be(MajorDeviceClassId::Uncategorized);
            }
        }
        buffer.write_le(bitfield);
    }
}

impl Exstruct<LittleEndian> for ClassOfDevice {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        let mut bitfield = BitBuffer::<u24>::new::<LittleEndian, B>(buffer)?;
        bitfield.set_range(13, 24);
        let service_classes: MajorServiceClasses = bitfield.read_be()?;
        bitfield.set_range(8, 13);
        let id: MajorDeviceClassId = bitfield.read_be()?;
        bitfield.set_range(2, 8);
        let device_class = match id {
            MajorDeviceClassId::Miscellaneous => DeviceClass::Miscellaneous,
            MajorDeviceClassId::Computer => DeviceClass::Computer(bitfield.read_be()?),
            MajorDeviceClassId::Phone => DeviceClass::Phone(bitfield.read_be()?),
            MajorDeviceClassId::LanAccessPoint => DeviceClass::LanAccessPoint(bitfield.read_be()?),
            MajorDeviceClassId::AudioVideo => DeviceClass::AudioVideo(bitfield.read_be()?),
            MajorDeviceClassId::Peripheral => DeviceClass::Peripheral(bitfield.read_be()?),
            MajorDeviceClassId::Imaging => DeviceClass::Imaging(bitfield.read_be()?),
            MajorDeviceClassId::Wearable => DeviceClass::Wearable(bitfield.read_be()?),
            MajorDeviceClassId::Toy => DeviceClass::Toy(bitfield.read_be()?),
            MajorDeviceClassId::Health => DeviceClass::Health(bitfield.read_be()?),
            MajorDeviceClassId::Uncategorized => DeviceClass::Uncategorized
        };
        Ok(ClassOfDevice {
            service_classes,
            device_class
        })
    }
}

impl Debug for ClassOfDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClassOfDevice")
            .field(
                "service_classes",
                &DebugFn(|f| bitflags::parser::to_writer(&self.service_classes, f))
            )
            .field("device_class", &self.device_class)
            .finish()
    }
}

bitflags! {
    /// Major Service Classes ([Assigned Numbers] Section 2.8.1).
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
    #[instructor(bitflags)]
    pub struct MajorServiceClasses: u16 {
        const LimitedDiscoverableMode = 0x0001;
        const LeAudio = 0x0002;
        const Positioning = 0x0008;
        const Networking = 0x0010;
        const Rendering = 0x0020;
        const Capturing = 0x0040;
        const ObjectTransfer = 0x0080;
        const Audio = 0x0100;
        const Telephony = 0x0200;
        const Information = 0x0400;
    }
}



#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum DeviceClass {
    Miscellaneous,
    Computer(ComputerClass),
    Phone(PhoneClass),
    LanAccessPoint(LanClass),
    AudioVideo(AudioVideoClass),
    Peripheral(PeripheralClass),
    Imaging(ImagingClass),
    Wearable(WearableClass),
    Toy(ToyClass),
    Health(HealthClass),
    Uncategorized
}

// ([Assigned Numbers] Section 2.8.2.1).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum ComputerClass {
    Uncategorized = 0b000,
    Desktop = 0b001,
    Server = 0b010,
    Laptop = 0b011,
    Handheld = 0b100,
    PalmSized = 0b101,
    Wearable = 0b110,
    Tablet = 0b111
}

// ([Assigned Numbers] Section 2.8.2.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum PhoneClass {
    Uncategorized = 0b000,
    Cellular = 0b001,
    Cordless = 0b010,
    Smartphone = 0b011,
    Modem = 0b100,
    Isdn = 0b101
}

// ([Assigned Numbers] Section 2.8.2.3).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum LanClass {
    FullyAvailable = 0b000000,
    UtilizedFrom1to17Percent = 0b001000,
    UtilizedFrom17to33Percent = 0b010000,
    UtilizedFrom33to50Percent = 0b011000,
    UtilizedFrom50to67Percent = 0b100000,
    UtilizedFrom67to83Percent = 0b101000,
    UtilizedFrom83to99Percent = 0b110000,
    NoServiceAvailable = 0b111000
}

// ([Assigned Numbers] Section 2.8.2.4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum AudioVideoClass {
    Uncategorized = 0b00000,
    WearableHeadset = 0b00001,
    HandsFree = 0b00010,
    Microphone = 0b00100,
    Loudspeaker = 0b00101,
    Headphones = 0b00110,
    PortableAudio = 0b00111,
    CarAudio = 0b01000,
    SetTopBox = 0b01001,
    HiFiAudio = 0b01010,
    Vcr = 0b01011,
    VideoCamera = 0b01100,
    Camcorder = 0b01101,
    VideoMonitor = 0b01110,
    VideoDisplayAndLoudspeaker = 0b01111,
    VideoConferencing = 0b10000,
    GamingToy = 0b10010
}

// ([Assigned Numbers] Section 2.8.2.5).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
pub struct  PeripheralClass {
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..5))]
    keyboard: bool,
    #[instructor(bits(5..6))]
    pointing_device: bool,
    #[instructor(bits(0..4))]
    device_type: PeripheralDeviceType
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum PeripheralDeviceType {
    Uncategorized = 0b0000,
    Joystick = 0b0001,
    Gamepad = 0b0010,
    RemoteControl = 0b0011,
    SensingDevice = 0b0100,
    DigitizerTablet = 0b0101,
    CardReader = 0b0110,
    DigitalPen = 0b0111,
    HandheldScanner = 0b1000,
    HandheldGestureInputDevice = 0b1001
}

bitflags! {
    /// ([Assigned Numbers] Section 2.8.2.6).
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
    #[instructor(bitflags)]
    pub struct ImagingClass: u8 {
        const Display = 0b000100;
        const Camera  = 0b001000;
        const Scanner = 0b010000;
        const Printer = 0b100000;
    }
}

// ([Assigned Numbers] Section 2.8.2.7).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum WearableClass {
    WristWatch = 0b001,
    Pager = 0b010,
    Jacket = 0b011,
    Helmet = 0b100,
    Glasses = 0b101,
    Pin = 0b110,
}

// ([Assigned Numbers] Section 2.8.2.8).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum ToyClass {
    Robot = 0b001,
    Vehicle = 0b010,
    Doll = 0b011,
    Controller = 0b100,
    Game = 0b101
}

// ([Assigned Numbers] Section 2.8.2.9).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum HealthClass {
    Undefined = 0b0000,
    BloodPressureMonitor = 0b0001,
    Thermometer = 0b0010,
    WeighingScale = 0b0011,
    GlucoseMeter = 0b0100,
    PulseOximeter = 0b0101,
    HeartRateMonitor = 0b0110,
    HealthDataDisplay = 0b0111,
    StepCounter = 0b1000,
    BodyCompositionAnalyzer = 0b1001,
    PeakFlowMonitor = 0b1010,
    MedicationMonitor = 0b1011,
    KneeProsthesis = 0b1100,
    AnkleProsthesis = 0b1101,
    GenericHealthManager = 0b1110,
    PersonalMobilityDevice = 0b1111
}

#[cfg(test)]
mod tests {
    use bytes::{Buf, Bytes, BytesMut};
    use super::*;

    #[test]
    fn test_cod() {
        let data: &[u8] = &[0x04, 0x04, 0x24];
        let mut bytes = Bytes::from_static(&data);
        let expected = ClassOfDevice {
            service_classes: MajorServiceClasses::Audio | MajorServiceClasses::Rendering,
            device_class: DeviceClass::AudioVideo(AudioVideoClass::WearableHeadset)
        };
        let cod: ClassOfDevice = bytes.read().unwrap();
        assert_eq!(cod, expected);
        let mut buffer = BytesMut::new();
        buffer.write(expected);
        assert_eq!(buffer.chunk(), data);
    }
}