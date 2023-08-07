fn main() {
    // Initialize the HIDAPI library
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            println!("Failed to create HID API instance: {}", e);
            return;
        }
    };

    // api.device_list().for_each(|device| {
    //     let name = device.manufacturer_string().unwrap_or("");
    //     let name = if name.is_empty() {
    //         device.product_string().unwrap_or("").to_string()
    //     } else {
    //         format!("{} {}", name, device.product_string().unwrap_or(""))
    //     };

    //     println!(
    //         "- {}. {}: {}",
    //         device.interface_number(),
    //         device.product_id(),
    //         name
    //     );
    // });

    // Vendor ID and Product ID for Logitech Pro Superlight
    let vendor_id = 0x046d; // Logitech's Vendor ID in hexadecimal
    let product_id = 0xc547; // Product ID for the Pro Superlight in hexadecimal

    // Try to open the HID device using Vendor ID and Product ID
    match api.open(vendor_id, product_id) {
        Ok(device) => {
            println!("Device Info:");
            println!("  Manufacturer: {:?}", device.get_manufacturer_string());
            println!("  Product Name: {:?}", device.get_product_string());
            println!("  Serial Number: {:?}", device.get_serial_number_string());

            const REPORT_ID_HIDPP_SHORT: u8 = 0x10;
            #[allow(unused)]
            const FEATURE_FEATURE_SET: u8 = 0x01;
            #[allow(unused)]
            const FEATURE_DEVICE_NAME: u8 = 0x03;
            #[allow(unused)]
            const FEATURE_UNIFIED_BATTERY: u8 = 0x06;

            let res = device.write(&[
                REPORT_ID_HIDPP_SHORT,
                0x01,
                FEATURE_DEVICE_NAME,
                0x00,
                0x06,
                0x00,
                0x00,
                0x00,
                0x00,
                0x00,
            ]);

            let mut buf = [0u8; 20];
            device.read(&mut buf).unwrap();

            // let discharge = buf[4];
            // let level = buf[5];

            let hex: String = buf.iter().map(|byte| format!("{:02X} ", byte)).collect();
            let chars: String = buf
                .iter()
                .map(|&byte| {
                    format!(
                        " {} ",
                        if (32..=126).contains(&byte) {
                            byte as char
                        } else {
                            '.'
                        }
                    )
                })
                .collect();

            println!("Write Result: {:?}", res);
            println!("Battery Status: {}", hex);
            println!("Battery Status: {}", chars);

            let discharge = buf[4];
            let level = buf[5];
            let status = buf[6];

            let status = match status {
                0x00 => "Discharging",
                0x01 => "Recharging",
                0x02 => "Almost Full",
                0x03 => "Full",
                0x04 => "Slow Recharge",
                0x05 => "Invalid Battery",
                0x06 => "Thermal Error",
                _ => "Unknown",
            };

            println!("Discharge: {}%", discharge);
            println!("Level: {}", level);
            println!("Status: {}", status);
        }
        Err(e) => {
            println!(
                "Failed to open device with VID: {:#x}, PID: {:#x}. Error: {}",
                vendor_id, product_id, e
            );
        }
    }
}
