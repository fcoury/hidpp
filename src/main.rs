#![allow(unused)]

use std::collections::HashMap;

use anyhow::{anyhow, bail};
use enum_iterator::{all, Sequence};
use retry::{delay::Fixed, retry, retry_with_index, OperationResult};
use tracing::Level;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("info"))
                .unwrap(),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let mut device = Device::new(0x046d, 0xc547).unwrap();
    device.init();

    let battery = device.get_battery_level().unwrap();
    println!("Battery: {}%", battery);
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Sequence)]
enum Feature {
    Root,
    FeatureSet,
    FeatureInfo,
    FirmwareInfo,
    DeviceUnitId,
    DeviceNameType,
    BatteryLevelStatus,
    UnifiedBattery,
}

impl Feature {
    fn value(&self) -> u16 {
        match self {
            Feature::Root { .. } => 0x0000,
            Feature::FeatureSet => 0x0001,
            Feature::FeatureInfo => 0x0002,
            Feature::FirmwareInfo => 0x0003,
            Feature::DeviceUnitId => 0x0004,
            Feature::DeviceNameType => 0x0005,
            Feature::BatteryLevelStatus => 0x1000,
            Feature::UnifiedBattery => 0x1004,
        }
    }
}

enum Function {
    RootGetFeature,
    RootGetProtocolVersion,
    UnifiedBatteryGetCapabilities,
    UnifiedBatteryGetStatus,
}

impl Function {
    fn value(&self) -> u8 {
        match self {
            Function::RootGetFeature => 0x00,
            Function::RootGetProtocolVersion => 0x01,
            Function::UnifiedBatteryGetCapabilities => 0x00,
            Function::UnifiedBatteryGetStatus => 0x01,
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
    pub fn send(&self, device: &mut Device) -> anyhow::Result<Message> {
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

        let buf = device.write(&buf)?;
        Message::try_from(buf.to_vec())
    }

    pub fn dump(&self) -> String {
        hexdump(self.data.clone(), 4)
        // format!(
        //     "report_id: {:?}, device_index: {:X}, feature_index: {:X}, function_index: {:X}, software_id: {:X}\ndata: {}",
        //     self.report_id, self.device_index, self.feature_index, self.function_index, self.software_id, hexdump(self.data.clone(), 4)
        // )
    }
}

impl TryFrom<Vec<u8>> for Message {
    type Error = anyhow::Error;

    fn try_from(buf: Vec<u8>) -> anyhow::Result<Self> {
        Ok(Self {
            report_id: match buf[0] {
                0x10 => ReportId::Short,
                0x11 => ReportId::Long,
                0x12 => ReportId::VeryLong,
                id => bail!("Invalid report id: 0x{:X}", id),
            },
            device_index: buf[1],
            feature_index: buf[2],
            function_index: buf[3] >> 4,
            software_id: buf[3] & 0x0F,
            data: buf[4..].to_vec(),
        })
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
    vendor_id: u16,
    product_id: u16,
    device: hidapi::HidDevice,
    features_index: HashMap<Feature, u8>,
}

impl Device {
    fn open(vendor_id: u16, product_id: u16) -> anyhow::Result<hidapi::HidDevice> {
        retry_with_index(
            Fixed::from_millis(10),
            |attempt| match hidapi::HidApi::new().unwrap().open(vendor_id, product_id) {
                Ok(device) => OperationResult::Ok(device),
                Err(err) => {
                    if attempt > 5 {
                        tracing::debug!("Giving up opening device: {}", err);
                        return OperationResult::Err(format!("Error opening device: {}", err));
                    }
                    tracing::debug!("Error opening device: {}", err);
                    OperationResult::Retry(format!("Error opening device: {}", err))
                }
            },
        )
        .map_err(|e| anyhow::anyhow!("Failed to open device: {}", e))
    }

    pub fn new(vendor_id: u16, product_id: u16) -> anyhow::Result<Self> {
        let device = Device::open(vendor_id, product_id)?;

        Ok(Device {
            vendor_id,
            product_id,
            device,
            features_index: HashMap::new(),
        })
    }

    pub fn reconnect(&mut self) -> anyhow::Result<()> {
        self.device = Device::open(self.vendor_id, self.product_id)?;
        Ok(())
    }

    pub fn init(&mut self) {
        let mut features_index = HashMap::from([(Feature::Root, 0x00u8)]);
        for feature in all::<Feature>().collect::<Vec<_>>() {
            let feature_index = self.get_feature_index(feature.clone()).unwrap();
            features_index.insert(feature, feature_index);
        }

        tracing::debug!("{:#?}", features_index);
        self.features_index = features_index;
    }

    pub fn write(&mut self, buf: &[u8]) -> anyhow::Result<Vec<u8>> {
        retry_with_index(
            Fixed::from_millis(1),
            |attempt| -> OperationResult<Vec<u8>, String> {
                match self.device.write(buf) {
                    Ok(_) => OperationResult::Ok(vec![]),
                    Err(e) => {
                        if attempt > 5 {
                            return OperationResult::Err(format!("Error writing to device: {}", e));
                        }
                        tracing::debug!("Error writing to device: {}", e);
                        self.reconnect();
                        OperationResult::Retry(format!("Error writing to device: {}", e))
                    }
                }
            },
        )
        .expect("Failed to write to device");
        tracing::trace!("Done writing");

        let mut buf = [0u8; 7];
        self.device.read_timeout(&mut buf, 100)?;
        Ok(buf.to_vec())
    }

    pub fn get_feature_index(&mut self, feature: Feature) -> anyhow::Result<u8> {
        let request = MessageBuilder::new_short(0x00, Function::RootGetFeature)
            .device_index(0x01)
            .add_u16(feature.value())
            .build();
        tracing::debug!("REQ {:?}: {}", feature, request.dump());
        let response = request.send(self).unwrap();
        tracing::debug!("RES {:?}: {}", feature, response.dump());
        tracing::debug!("");
        Ok(response.data[0])
    }

    pub fn index_for(&self, feature: Feature) -> anyhow::Result<u8> {
        self.features_index
            .get(&feature)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Feature {:?} not found", feature))
    }

    pub fn send_feature(
        &mut self,
        feature: Feature,
        function: Function,
        payload: &[u8],
    ) -> anyhow::Result<Message> {
        let request = MessageBuilder::new_short(self.index_for(feature.clone())?, function)
            .device_index(0x01)
            .data(payload.to_vec())
            .build();
        tracing::debug!("REQ {:?}: {}", feature, request.dump());
        let response = request.send(self).unwrap();
        tracing::debug!("RES {:?}: {}", feature, response.dump());
        tracing::debug!("");
        Ok(response)
    }

    pub fn get_battery_level(&mut self) -> anyhow::Result<u8> {
        let result = self.send_feature(
            Feature::UnifiedBattery,
            Function::UnifiedBatteryGetCapabilities,
            &[],
        )?;
        tracing::debug!("Capabilities: {}", result.dump());

        let result = self.send_feature(
            Feature::UnifiedBattery,
            Function::UnifiedBatteryGetStatus,
            &[],
        )?;
        tracing::debug!("Battery level: {}", result.dump());
        Ok(result.data[0])
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
