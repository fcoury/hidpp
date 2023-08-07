#![allow(unused)]
fn main() {
    let report = HidppReportBuilder::new_short()
        .feature_id()
}

enum Feature {
    IRoot,
    IFeatureSet,
    IFirmwareInfo,
    GetDeviceNameType,
    BatteryLevelStatus,
}

impl Feature {
    fn to_u16(&self) -> u16 {
        match self {
            Feature::IRoot => 0x0000,
            Feature::IFeatureSet => 0x0001,
            Feature::IFirmwareInfo => 0x0003,
            Feature::GetDeviceNameType => 0x0005,
            Feature::BatteryLevelStatus => 0x1000,
        }
    }
}

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
struct HidppReport {
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

impl HidppReport {
    pub fn send(&self, device: &HidppDevice) -> Result<(), String> {
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

        Ok(())
    }
}

struct HidppReportBuilder {
    report_id: ReportId,
    device_index: u8,
    feature_id: u8,
    feature_index: u8,
    software_id: u8,
    data: Vec<u8>,
}

impl HidppReportBuilder {
    pub fn new_short() -> Self {
        Self {
            report_id: ReportId::Short,
            device_index: 0x01,
            feature_id: 0x03,
            feature_index: 0x00,
            software_id: 0x06,
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

    pub fn feature_id(mut self, feature_id: u8) -> Self {
        self.feature_id = feature_id;
        self
    }

    pub fn feature_index(mut self, feature_index: u8) -> Self {
        self.feature_index = feature_index;
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

    pub fn build(self) -> HidppReport {
        HidppReport {
            report_id: self.report_id,
            device_index: self.device_index,
            feature_id: self.feature_id,
            feature_index: self.feature_index,
            software_id: self.software_id,
            data: self.data,
        }
    }
}

struct HidppDevice {
    device: hidapi::HidDevice,
}

impl HidppDevice {
    pub fn query_battery(&self) {
        let report = HidppReport {
            report_id: ReportId::Short,
            device_index: 0x01,
            feature_id: 0x03,
            feature_index: 0x00,
            software_id: 0x06,
            data: vec![],
        };

        report.send(&self);
    }
}
