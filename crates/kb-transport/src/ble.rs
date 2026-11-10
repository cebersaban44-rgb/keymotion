//! Read-only Bluetooth LE access through WinRT.
//!
//! Windows owns the HID-over-GATT service of a paired keyboard, so the only
//! useful data reachable here is the standard Battery Service (0x180F).
//! There is deliberately no write API: the keyboard's vendor GATT service
//! (0xFEE0/0xFEE1) is the radio chip's firmware-update channel.

use crate::{Result, TransportError};
use windows::core::HSTRING;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristicUuids, GattCommunicationStatus, GattServiceUuids,
};
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Storage::Streams::DataReader;

/// A paired Bluetooth LE device.
#[derive(Debug, Clone)]
pub struct BleDevice {
    pub name: String,
    /// 48-bit Bluetooth address.
    pub address: u64,
    pub connected: bool,
    pub id: String,
}

impl BleDevice {
    pub fn address_string(&self) -> String {
        let a = self.address.to_be_bytes();
        format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", a[2], a[3], a[4], a[5], a[6], a[7])
    }
}

/// All Bluetooth LE devices paired with this PC.
pub fn paired_devices() -> Result<Vec<BleDevice>> {
    let selector = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
    let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector)?.join()?;
    let mut out = Vec::new();
    for info in infos {
        let id: HSTRING = info.Id()?;
        let Ok(device) = BluetoothLEDevice::FromIdAsync(&id).and_then(|op| op.join()) else {
            continue;
        };
        out.push(BleDevice {
            name: device.Name()?.to_string_lossy(),
            address: device.BluetoothAddress()?,
            connected: device.ConnectionStatus()? == BluetoothConnectionStatus::Connected,
            id: id.to_string_lossy(),
        });
        device.Close()?;
    }
    Ok(out)
}

/// Read the Battery Level characteristic (0x2A19), 0–100 %.
pub fn read_battery(address: u64) -> Result<u8> {
    let device = BluetoothLEDevice::FromBluetoothAddressAsync(address)?.join()?;
    let result = (|| -> Result<u8> {
        let services = device
            .GetGattServicesForUuidWithCacheModeAsync(GattServiceUuids::Battery()?, BluetoothCacheMode::Uncached)?
            .join()?;
        gatt_ok(services.Status()?, "battery service")?;
        let service = services
            .Services()?
            .into_iter()
            .next()
            .ok_or_else(|| TransportError::Bluetooth("no battery service".into()))?;
        let read = (|| -> Result<u8> {
            let chars = service
                .GetCharacteristicsForUuidWithCacheModeAsync(
                    GattCharacteristicUuids::BatteryLevel()?,
                    BluetoothCacheMode::Uncached,
                )?
                .join()?;
            gatt_ok(chars.Status()?, "battery characteristic")?;
            let ch = chars
                .Characteristics()?
                .into_iter()
                .next()
                .ok_or_else(|| TransportError::Bluetooth("no battery level characteristic".into()))?;
            let value = ch.ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)?.join()?;
            gatt_ok(value.Status()?, "battery read")?;
            let reader = DataReader::FromBuffer(&value.Value()?)?;
            Ok(reader.ReadByte()?)
        })();
        service.Close()?;
        read
    })();
    device.Close()?;
    result
}

fn gatt_ok(status: GattCommunicationStatus, what: &str) -> Result<()> {
    if status == GattCommunicationStatus::Success {
        Ok(())
    } else {
        Err(TransportError::Bluetooth(format!("{what}: GATT status {}", status.0)))
    }
}
