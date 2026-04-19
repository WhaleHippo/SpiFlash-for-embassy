#![no_std]
#![no_main]

use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, peripherals};
use embassy_time::{Delay, Duration, Timer};
use spi_flash_for_embassy::SpiFlash;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    DMA2_STREAM3 => embassy_stm32::dma::InterruptHandler<peripherals::DMA2_CH3>;
    DMA2_STREAM2 => embassy_stm32::dma::InterruptHandler<peripherals::DMA2_CH2>;
});

type FlashSpi = Spi<'static, Async, embassy_stm32::spi::mode::Master>;
type Led = Output<'static>;
type CsPin = Output<'static>;

const TEST_PATTERN: &[u8] = b"SPI flash Embassy STM32F401RE example";

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    let mut spi_config = SpiConfig::default();
    spi_config.frequency = Hertz::mhz(8);
    spi_config.mode = embassy_stm32::spi::MODE_0;

    let spi: FlashSpi = Spi::new(
        p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA2_CH3, p.DMA2_CH2, Irqs, spi_config,
    );

    let cs: CsPin = Output::new(p.PB6, Level::High, Speed::VeryHigh);
    let mut led: Led = Output::new(p.PB0, Level::High, Speed::Low);
    let delay = Delay;
    let mut flash = SpiFlash::new(spi, cs, delay);

    info!("STM32F401RE SPI flash example start");
    info!("Assumed wiring: SPI1 SCK=PA5 MISO=PA6 MOSI=PA7 CS=PB6 LED=PB0");

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
            blink_error(&mut led).await;
        }
    };

    if device.sector_count == 0 {
        error!("detected flash has no sectors");
        blink_error(&mut led).await;
    }

    if let Err(err) = flash.erase_sector(0).await {
        error!("erase_sector(0) failed: {:?}", err);
        blink_error(&mut led).await;
    }

    if let Err(err) = flash.write_address(0, TEST_PATTERN).await {
        error!("write_address failed: {:?}", err);
        blink_error(&mut led).await;
    }

    let mut readback = [0u8; TEST_PATTERN.len()];
    if let Err(err) = flash.read_address(0, &mut readback).await {
        error!("read_address failed: {:?}", err);
        blink_error(&mut led).await;
    }

    if readback.as_slice() == TEST_PATTERN {
        info!(
            "readback matched test pattern ({} bytes)",
            TEST_PATTERN.len()
        );
    } else {
        warn!("readback mismatch");
        blink_error(&mut led).await;
    }

    loop {
        led.toggle();
        Timer::after(Duration::from_millis(500)).await;
    }
}

async fn blink_error(led: &mut Led) -> ! {
    loop {
        led.set_low();
        Timer::after_millis(100).await;
        led.set_high();
        Timer::after_millis(100).await;
    }
}
