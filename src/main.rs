#![allow(unused)]

use std::collections::HashMap;

use enum_iterator::{all, Sequence};
fn main() {
    let device = Device::new(0x046d, 0xc547).unwrap();
    let mut features_index = HashMap::from([(Feature::IRoot, 0x00u8)]);
    for feature in all::<Feature>().collect::<Vec<_>>() {
        let request = MessageBuilder::new_short(
            *features_index.get(&Feature::IRoot).unwrap(),
            Function::GetFeature,
        )
        .device_index(0x01)
        .add_u16(feature.value())
        .build();
        println!("REQ {:?}: {}", feature, request.dump());
        let response = request.send(&device).unwrap();
        println!("RES {:?}: {}", feature, response.dump());
        features_index.insert(feature, response.data[0]);
        println!();
    }
    // let request = MessageBuilder::new_short(
    //     *features_index.get(&Feature::IRoot).unwrap(),
    //     Function::GetFeature,
    // )
    // .device_index(0x01)
    // .add_u16(Feature::IFeatureSet.value())
    // .build();
    // println!("{}", request.dump());
    // let response = request.send(&device).unwrap();
    // println!("{}", response.dump());
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Sequence)]
enum Feature {
    IRoot,
    IFeatureSet,
    IFirmwareInfo,
    GetDeviceNameType,
    BatteryLevelStatus,
    UnifiedBattery,
}

impl Feature {
    fn value(&self) -> u16 {
        match self {
            Feature::IRoot { .. } => 0x0000,
            Feature::IFeatureSet => 0x0001,
            Feature::IFirmwareInfo => 0x0003,
            Feature::GetDeviceNameType => 0x0005,
            Feature::BatteryLevelStatus => 0x1000,
            Feature::UnifiedBattery => 0x1004,
        }
    }
}

enum Function {
    GetFeature,
    GetProtocolVersion,
}

impl Function {
    fn value(&self) -> u8 {
        match self {
            Function::GetFeature => 0x00,
            Function::GetProtocolVersion => 0x01,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum ReportId {
    Short,
    Long,
    VeryLong,
}

impl ReportId {
    fn to_u8(&self) -> u8 {
        match self {
            ReportId::Short => 0x10,
            ReportId::Long => 0x11,
            ReportId::VeryLong => 0x12,
        }
    }
}

// ping is 10 00 00 10 00 00 AA
// 10 = report_id
// 00 = device_index
// 00 = feature_index
// 10 = function_index (0x01 = ping) and software_id (0x00 = unknown)
// 00 00 AA = data
struct Message {
    // byte 0 - the report id (Short, Long or VeryLong)
    report_id: ReportId,
    // byte 1 - 0xff until the device is known, then the device index
    device_index: u8,
    // byte 2 - the feature index based on querying the feature (0x1000 is 0x06 for instance)
    feature_index: u8,
    // byte 3a - constitutes the MSB of the Fcnt/ASE + Sw Id. byte (3rd)
    // it's the function index for the feature
    function_index: u8,
    // byte 3b - constitutes the LSB of the Fcnt/ASE + Sw Id. byte (3rd)
    // it's the software attributed id, must be non-zero
    software_id: u8,
    // bytes 5-6 - payload
    data: Vec<u8>,
}

impl Message {
    pub fn send(&self, device: &Device) -> anyhow::Result<Message> {
        let mut buf = vec![
            self.report_id.to_u8(),
            self.device_index,
            self.feature_index,
            self.function_index << 4 | self.software_id & 0x0F,
        ];
        // appends data to buf, padding with 0 until the length of 7
        buf.extend(
            self.data
                .iter()
                .copied()
                .chain(std::iter::repeat(0))
                .take(7),
        );

        match device.device.write(&buf).map_err(|e| e.to_string()) {
            Ok(_) => {}
            Err(e) => {
                println!("Error writing to device: {}", e);
            }
        }

        let mut buf = [0u8; 7];
        let res = device.device.read_timeout(&mut buf, 1000)?;

        Ok(Message::from(buf.to_vec()))
    }

    pub fn dump(&self) -> String {
        hexdump(self.data.clone(), 4)
        // format!(
        //     "report_id: {:?}, device_index: {:X}, feature_index: {:X}, function_index: {:X}, software_id: {:X}\ndata: {}",
        //     self.report_id, self.device_index, self.feature_index, self.function_index, self.software_id, hexdump(self.data.clone(), 4)
        // )
    }
}

impl From<Vec<u8>> for Message {
    fn from(buf: Vec<u8>) -> Self {
        Self {
            report_id: match buf[0] {
                0x10 => ReportId::Short,
                0x11 => ReportId::Long,
                0x12 => ReportId::VeryLong,
                id => panic!("Invalid report id: 0x{:X}", id),
            },
            device_index: buf[1],
            feature_index: buf[2],
            function_index: buf[3] >> 4,
            software_id: buf[3] & 0x0F,
            data: buf[4..].to_vec(),
        }
    }
}

struct MessageBuilder {
    report_id: ReportId,
    device_index: u8,
    feature_index: u8,
    function_index: u8,
    software_id: u8,
    data: Vec<u8>,
}

impl MessageBuilder {
    pub fn new_short(feature_index: u8, function: Function) -> Self {
        Self {
            report_id: ReportId::Short,
            device_index: 0xff,
            feature_index,
            function_index: function.value(),
            software_id: 0x01,
            data: vec![],
        }
    }

    pub fn report_id(mut self, report_id: ReportId) -> Self {
        self.report_id = report_id;
        self
    }

    pub fn device_index(mut self, device_index: u8) -> Self {
        self.device_index = device_index;
        self
    }

    pub fn feature_index(mut self, feature_index: u8) -> Self {
        self.feature_index = feature_index;
        self
    }

    pub fn function_index(mut self, function_index: u8) -> Self {
        self.function_index = function_index;
        self
    }

    pub fn software_id(mut self, software_id: u8) -> Self {
        self.software_id = software_id;
        self
    }

    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub fn add_u16(mut self, data: u16) -> Self {
        self.data.extend_from_slice(&data.to_be_bytes());
        self
    }

    pub fn build(self) -> Message {
        // if self.data len is less than 3 then fill it up with 0x00
        let data = self
            .data
            .iter()
            .copied()
            .chain(std::iter::repeat(0))
            .take(3)
            .collect();
        Message {
            report_id: self.report_id,
            device_index: self.device_index,
            feature_index: self.feature_index,
            function_index: self.function_index,
            software_id: self.software_id,
            data,
        }
    }
}

struct Device {
    device: hidapi::HidDevice,
}

impl Device {
    pub fn new(vid: u16, pid: u16) -> anyhow::Result<Self> {
        let device = hidapi::HidApi::new()
            .unwrap()
            .open(vid, pid)
            .expect("Failed to open device");

        Ok(Device { device })
    }
}

fn hexdump(data: Vec<u8>, chunk_size: usize) -> String {
    let mut lines = String::new();
    for chunk in data.chunks(chunk_size) {
        let hex_part: Vec<String> = chunk.iter().map(|byte| format!("{:02x}", byte)).collect();
        let char_part: Vec<String> = chunk
            .iter()
            .map(|&byte| {
                if byte.is_ascii() && !byte.is_ascii_control() {
                    format!("{}", byte as char)
                } else {
                    ".".to_string()
                }
            })
            .collect();

        lines.push_str(&format!(
            "{:<width$}  {}",
            hex_part.join(" "),
            char_part.join(""),
            width = 3 * chunk_size
        ));
    }
    lines
}
