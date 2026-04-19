#![no_std]
#![no_main]

use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::spim::{self, Config as SpimConfig, Frequency, Spim};
use embassy_nrf::{bind_interrupts, peripherals};
use embassy_time::{Delay, Duration, Timer};
use spi_flash_for_embassy::SpiFlash;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    SERIAL3 => spim::InterruptHandler<peripherals::SERIAL3>;
});

type CsPin = Output<'static>;

const TEST_PATTERN: &[u8] = b"SPI flash Embassy nRF5340 example";

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    let mut config = SpimConfig::default();
    config.frequency = Frequency::M8;
    config.mode = spim::MODE_0;

    let spi = Spim::new(p.SERIAL3, Irqs, p.P0_28, p.P0_29, p.P0_30, config);
    let cs: CsPin = Output::new(p.P0_31, Level::High, OutputDrive::Standard);
    let delay = Delay;
    let mut flash = SpiFlash::new(spi, cs, delay);

    info!("nRF5340 application core SPI flash example start");
    info!("Assumed wiring: SPIM3 SCK=P0.28 MISO=P0.29 MOSI=P0.30 CS=P0.31");

    let device = match flash.initialize().await {
        Ok(device) => {
            info!(
                "flash ready: manufacturer={:?} type=0x{:02x} density=0x{:02x} capacity={} bytes",
                device.manufacturer,
                device.jedec_id.memory_type,
                device.jedec_id.capacity_code,
                device.capacity_bytes
            );
            device
        }
        Err(err) => {
            error!("flash init failed: {:?}", err);
            park_forever().await;
        }
    };

    if device.sector_count == 0 {
        error!("detected flash has no sectors");
        park_forever().await;
    }

    if let Err(err) = flash.erase_sector(0).await {
        error!("erase_sector(0) failed: {:?}", err);
        park_forever().await;
    }

    if let Err(err) = flash.write_address(0, TEST_PATTERN).await {
        error!("write_address failed: {:?}", err);
        park_forever().await;
    }

    let mut readback = [0u8; TEST_PATTERN.len()];
    if let Err(err) = flash.read_address(0, &mut readback).await {
        error!("read_address failed: {:?}", err);
        park_forever().await;
    }

    if readback.as_slice() == TEST_PATTERN {
        info!(
            "readback matched test pattern ({} bytes)",
            TEST_PATTERN.len()
        );
    } else {
        warn!("readback mismatch");
        park_forever().await;
    }

    loop {
        info!("SPI flash example heartbeat");
        Timer::after(Duration::from_secs(1)).await;
    }
}

async fn park_forever() -> ! {
    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}
