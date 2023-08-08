use std::collections::HashMap;

use anyhow::bail;
use enum_iterator::all;
use retry::{delay::Fixed, retry_with_index, OperationResult};

use crate::{Feature, Function, Message, MessageBuilder};

pub struct Device {
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
                        self.reconnect().unwrap();
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

    pub fn get_battery(&mut self) -> anyhow::Result<(u8, BatteryLevel, BatteryStatus)> {
        let result = self.send_feature(
            Feature::UnifiedBattery,
            Function::UnifiedBatteryGetStatus,
            &[],
        )?;
        tracing::debug!("Battery level: {}", result.dump());

        Ok((
            result.data[0],
            BatteryLevel::try_from(result.data[1])?,
            BatteryStatus::try_from(result.data[2])?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum BatteryStatus {
    Discharging,
    Recharging,
    AlmostFull,
    Full,
    SlowRecharge,
    InvalidBattery,
    ThermalError,
}

impl TryFrom<u8> for BatteryStatus {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> anyhow::Result<Self> {
        match value {
            0x00 => Ok(BatteryStatus::Discharging),
            0x01 => Ok(BatteryStatus::Recharging),
            0x02 => Ok(BatteryStatus::AlmostFull),
            0x03 => Ok(BatteryStatus::Full),
            0x04 => Ok(BatteryStatus::SlowRecharge),
            0x05 => Ok(BatteryStatus::InvalidBattery),
            0x06 => Ok(BatteryStatus::ThermalError),
            _ => bail!("Invalid battery status: 0x{:X}", value),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum BatteryLevel {
    Full,
    Good,
    Low,
    Critical,
    Empty,
}

impl TryFrom<u8> for BatteryLevel {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> anyhow::Result<Self> {
        match value {
            0 => Ok(BatteryLevel::Empty),
            1 => Ok(BatteryLevel::Critical),
            2..=3 => Ok(BatteryLevel::Low),
            4..=7 => Ok(BatteryLevel::Good),
            8 => Ok(BatteryLevel::Full),
            _ => bail!("Invalid battery level: 0x{:X}", value),
        }
    }
}
