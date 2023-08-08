use hidpp::Device;
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

    let (percentage, level, status) = device.get_battery().unwrap();
    println!("Battery: {}%", percentage);
    println!("Level: {:?}", level);
    println!("Status: {:?}", status);
}
