#![cfg_attr(not(test), no_std)]

use core::convert::Infallible;

use embedded_hal::digital::OutputPin;
use embedded_hal_async::{delay::DelayNs, spi::SpiBus};

pub const PAGE_SIZE: u32 = 256;
pub const SECTOR_SIZE: u32 = 4 * 1024;
pub const BLOCK_SIZE: u32 = 64 * 1024;

const CMD_READ_JEDEC_ID: u8 = 0x9F;
const CMD_WRITE_DISABLE: u8 = 0x04;
const CMD_READ_STATUS1: u8 = 0x05;
const CMD_WRITE_ENABLE: u8 = 0x06;
const CMD_PAGE_PROGRAM_3B: u8 = 0x02;
const CMD_PAGE_PROGRAM_4B: u8 = 0x12;
const CMD_READ_DATA_3B: u8 = 0x03;
const CMD_READ_DATA_4B: u8 = 0x13;
const CMD_SECTOR_ERASE_3B: u8 = 0x20;
const CMD_SECTOR_ERASE_4B: u8 = 0x21;
const CMD_BLOCK_ERASE_3B: u8 = 0xD8;
const CMD_BLOCK_ERASE_4B: u8 = 0xDC;
const CMD_CHIP_ERASE: u8 = 0x60;

const STATUS_BUSY: u8 = 1 << 0;
const INIT_DELAY_MS: u32 = 20;
const PAGE_PROGRAM_TIMEOUT_MS: u32 = 100;
const SECTOR_ERASE_TIMEOUT_MS: u32 = 1_000;
const BLOCK_ERASE_TIMEOUT_MS: u32 = 3_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Manufacturer {
    Unknown(u8),
    Winbond,
    Issi,
    Micron,
    GigaDevice,
    Macronix,
    Spansion,
    Amic,
    Sst,
    Hyundai,
    Atmel,
    Fudan,
    Esmt,
    Intel,
    Sanyo,
    Fujitsu,
    Eon,
    Puya,
}

impl Manufacturer {
    pub const fn from_jedec(code: u8) -> Self {
        match code {
            0xEF => Self::Winbond,
            0x9D => Self::Issi,
            0x20 => Self::Micron,
            0xC8 => Self::GigaDevice,
            0xC2 => Self::Macronix,
            0x01 => Self::Spansion,
            0x37 => Self::Amic,
            0xBF => Self::Sst,
            0xAD => Self::Hyundai,
            0x1F => Self::Atmel,
            0xA1 => Self::Fudan,
            0x8C => Self::Esmt,
            0x89 => Self::Intel,
            0x62 => Self::Sanyo,
            0x04 => Self::Fujitsu,
            0x1C => Self::Eon,
            0x85 => Self::Puya,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AddressMode {
    ThreeByte,
    FourByte,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct JedecId {
    pub manufacturer_code: u8,
    pub memory_type: u8,
    pub capacity_code: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DeviceInfo {
    pub jedec_id: JedecId,
    pub manufacturer: Manufacturer,
    pub address_mode: AddressMode,
    pub capacity_bytes: u32,
    pub page_count: u32,
    pub sector_count: u32,
    pub block_count: u32,
}

impl DeviceInfo {
    pub const fn page_address(&self, page: u32) -> u32 {
        page * PAGE_SIZE
    }

    pub const fn sector_address(&self, sector: u32) -> u32 {
        sector * SECTOR_SIZE
    }

    pub const fn block_address(&self, block: u32) -> u32 {
        block * BLOCK_SIZE
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<SpiError, PinError = Infallible> {
    Spi(SpiError),
    Pin(PinError),
    NotInitialized,
    UnsupportedCapacityCode(u8),
    OutOfBounds,
    BusyTimeout,
}

pub struct SpiFlash<SPI, CS, DELAY> {
    spi: SPI,
    cs: CS,
    delay: DELAY,
    device_info: Option<DeviceInfo>,
}

impl<SPI, CS, DELAY> SpiFlash<SPI, CS, DELAY> {
    pub const fn new(spi: SPI, cs: CS, delay: DELAY) -> Self {
        Self {
            spi,
            cs,
            delay,
            device_info: None,
        }
    }

    pub fn device_info(&self) -> Option<DeviceInfo>
    where
        DeviceInfo: Copy,
    {
        self.device_info
    }

    pub fn release(self) -> (SPI, CS, DELAY) {
        (self.spi, self.cs, self.delay)
    }
}

impl<SPI, CS, DELAY> SpiFlash<SPI, CS, DELAY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    DELAY: DelayNs,
{
    pub async fn initialize(&mut self) -> Result<DeviceInfo, Error<SPI::Error, CS::Error>> {
        self.cs.set_high().map_err(Error::Pin)?;
        self.delay.delay_ms(INIT_DELAY_MS).await;
        self.write_disable().await?;

        let jedec_id = self.read_jedec_id().await?;
        let device_info = device_info_from_jedec(jedec_id)?;
        self.device_info = Some(device_info);
        Ok(device_info)
    }

    pub async fn read_jedec_id(&mut self) -> Result<JedecId, Error<SPI::Error, CS::Error>> {
        let mut id = [0u8; 3];
        self.select().await?;
        let result = async {
            self.spi
                .write(&[CMD_READ_JEDEC_ID])
                .await
                .map_err(Error::Spi)?;
            self.spi.read(&mut id).await.map_err(Error::Spi)?;
            self.spi.flush().await.map_err(Error::Spi)?;
            Ok::<(), Error<SPI::Error, CS::Error>>(())
        }
        .await;
        self.deselect().await?;
        result?;

        Ok(JedecId {
            manufacturer_code: id[0],
            memory_type: id[1],
            capacity_code: id[2],
        })
    }

    pub async fn erase_chip(&mut self) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        self.write_enable().await?;
        self.send_simple_command(CMD_CHIP_ERASE).await?;
        self.wait_until_ready(info.block_count.saturating_mul(1_000))
            .await
    }

    pub async fn erase_sector(&mut self, sector: u32) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if sector >= info.sector_count {
            return Err(Error::OutOfBounds);
        }

        self.write_enable().await?;
        self.send_address_command(
            info.sector_address(sector),
            CMD_SECTOR_ERASE_3B,
            CMD_SECTOR_ERASE_4B,
        )
        .await?;
        self.wait_until_ready(SECTOR_ERASE_TIMEOUT_MS).await
    }

    pub async fn erase_block(&mut self, block: u32) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if block >= info.block_count {
            return Err(Error::OutOfBounds);
        }

        self.write_enable().await?;
        self.send_address_command(
            info.block_address(block),
            CMD_BLOCK_ERASE_3B,
            CMD_BLOCK_ERASE_4B,
        )
        .await?;
        self.wait_until_ready(BLOCK_ERASE_TIMEOUT_MS).await
    }

    pub async fn write_address(
        &mut self,
        address: u32,
        data: &[u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.ensure_range(address, data.len())?;

        let mut cursor = address;
        let mut remaining = data;
        while !remaining.is_empty() {
            let page_offset = (cursor % PAGE_SIZE) as usize;
            let writable = remaining.len().min((PAGE_SIZE as usize) - page_offset);
            let (head, tail) = remaining.split_at(writable);
            self.program_page(cursor, head).await?;
            cursor += writable as u32;
            remaining = tail;
        }

        Ok(())
    }

    pub async fn write_page(
        &mut self,
        page: u32,
        offset: u32,
        data: &[u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if page >= info.page_count || offset >= PAGE_SIZE || data.len() as u32 > PAGE_SIZE - offset
        {
            return Err(Error::OutOfBounds);
        }

        self.program_page(info.page_address(page) + offset, data)
            .await
    }

    pub async fn write_sector(
        &mut self,
        sector: u32,
        offset: u32,
        data: &[u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if sector >= info.sector_count
            || offset > SECTOR_SIZE
            || data.len() as u32 > SECTOR_SIZE - offset
        {
            return Err(Error::OutOfBounds);
        }

        self.write_address(info.sector_address(sector) + offset, data)
            .await
    }

    pub async fn write_block(
        &mut self,
        block: u32,
        offset: u32,
        data: &[u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if block >= info.block_count
            || offset > BLOCK_SIZE
            || data.len() as u32 > BLOCK_SIZE - offset
        {
            return Err(Error::OutOfBounds);
        }

        self.write_address(info.block_address(block) + offset, data)
            .await
    }

    pub async fn read_address(
        &mut self,
        address: u32,
        data: &mut [u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.ensure_range(address, data.len())?;
        if data.is_empty() {
            return Ok(());
        }

        let info = self.require_info()?;
        let (header, header_len) = build_address_header(
            info.address_mode,
            CMD_READ_DATA_3B,
            CMD_READ_DATA_4B,
            address,
        );
        self.select().await?;
        let result = async {
            self.spi
                .write(&header[..header_len])
                .await
                .map_err(Error::Spi)?;
            self.spi.read(data).await.map_err(Error::Spi)?;
            self.spi.flush().await.map_err(Error::Spi)?;
            Ok::<(), Error<SPI::Error, CS::Error>>(())
        }
        .await;
        self.deselect().await?;
        result
    }

    pub async fn read_page(
        &mut self,
        page: u32,
        offset: u32,
        data: &mut [u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if page >= info.page_count || offset > PAGE_SIZE || data.len() as u32 > PAGE_SIZE - offset {
            return Err(Error::OutOfBounds);
        }

        self.read_address(info.page_address(page) + offset, data)
            .await
    }

    pub async fn read_sector(
        &mut self,
        sector: u32,
        offset: u32,
        data: &mut [u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if sector >= info.sector_count
            || offset > SECTOR_SIZE
            || data.len() as u32 > SECTOR_SIZE - offset
        {
            return Err(Error::OutOfBounds);
        }

        self.read_address(info.sector_address(sector) + offset, data)
            .await
    }

    pub async fn read_block(
        &mut self,
        block: u32,
        offset: u32,
        data: &mut [u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        if block >= info.block_count
            || offset > BLOCK_SIZE
            || data.len() as u32 > BLOCK_SIZE - offset
        {
            return Err(Error::OutOfBounds);
        }

        self.read_address(info.block_address(block) + offset, data)
            .await
    }

    async fn program_page(
        &mut self,
        address: u32,
        data: &[u8],
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.ensure_range(address, data.len())?;
        if data.is_empty() {
            return Ok(());
        }
        if crosses_boundary(address, data.len(), PAGE_SIZE) {
            return Err(Error::OutOfBounds);
        }

        let info = self.require_info()?;
        let (header, header_len) = build_address_header(
            info.address_mode,
            CMD_PAGE_PROGRAM_3B,
            CMD_PAGE_PROGRAM_4B,
            address,
        );

        self.write_enable().await?;
        self.select().await?;
        let result = async {
            self.spi
                .write(&header[..header_len])
                .await
                .map_err(Error::Spi)?;
            self.spi.write(data).await.map_err(Error::Spi)?;
            self.spi.flush().await.map_err(Error::Spi)?;
            Ok::<(), Error<SPI::Error, CS::Error>>(())
        }
        .await;
        self.deselect().await?;
        result?;
        self.wait_until_ready(PAGE_PROGRAM_TIMEOUT_MS).await
    }

    async fn write_enable(&mut self) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.send_simple_command(CMD_WRITE_ENABLE).await
    }

    async fn write_disable(&mut self) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.send_simple_command(CMD_WRITE_DISABLE).await
    }

    async fn read_status1(&mut self) -> Result<u8, Error<SPI::Error, CS::Error>> {
        let mut status = [0u8; 1];
        self.select().await?;
        let result = async {
            self.spi
                .write(&[CMD_READ_STATUS1])
                .await
                .map_err(Error::Spi)?;
            self.spi.read(&mut status).await.map_err(Error::Spi)?;
            self.spi.flush().await.map_err(Error::Spi)?;
            Ok::<(), Error<SPI::Error, CS::Error>>(())
        }
        .await;
        self.deselect().await?;
        result?;
        Ok(status[0])
    }

    async fn wait_until_ready(
        &mut self,
        timeout_ms: u32,
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let mut waited_ms = 0u32;
        loop {
            if self.read_status1().await? & STATUS_BUSY == 0 {
                return Ok(());
            }
            if waited_ms >= timeout_ms {
                return Err(Error::BusyTimeout);
            }
            self.delay.delay_ms(1).await;
            waited_ms = waited_ms.saturating_add(1);
        }
    }

    async fn send_simple_command(
        &mut self,
        command: u8,
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.select().await?;
        let result = async {
            self.spi.write(&[command]).await.map_err(Error::Spi)?;
            self.spi.flush().await.map_err(Error::Spi)?;
            Ok::<(), Error<SPI::Error, CS::Error>>(())
        }
        .await;
        self.deselect().await?;
        result
    }

    async fn send_address_command(
        &mut self,
        address: u32,
        command_3b: u8,
        command_4b: u8,
    ) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        let (header, header_len) =
            build_address_header(info.address_mode, command_3b, command_4b, address);
        self.select().await?;
        let result = async {
            self.spi
                .write(&header[..header_len])
                .await
                .map_err(Error::Spi)?;
            self.spi.flush().await.map_err(Error::Spi)?;
            Ok::<(), Error<SPI::Error, CS::Error>>(())
        }
        .await;
        self.deselect().await?;
        result
    }

    async fn select(&mut self) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.cs.set_low().map_err(Error::Pin)
    }

    async fn deselect(&mut self) -> Result<(), Error<SPI::Error, CS::Error>> {
        self.cs.set_high().map_err(Error::Pin)
    }

    fn require_info(&self) -> Result<DeviceInfo, Error<SPI::Error, CS::Error>> {
        self.device_info.ok_or(Error::NotInitialized)
    }

    fn ensure_range(&self, address: u32, len: usize) -> Result<(), Error<SPI::Error, CS::Error>> {
        let info = self.require_info()?;
        let end = address.checked_add(len as u32).ok_or(Error::OutOfBounds)?;
        if end > info.capacity_bytes {
            return Err(Error::OutOfBounds);
        }
        Ok(())
    }
}

fn build_address_header(
    address_mode: AddressMode,
    command_3b: u8,
    command_4b: u8,
    address: u32,
) -> ([u8; 5], usize) {
    match address_mode {
        AddressMode::ThreeByte => (
            [
                command_3b,
                ((address >> 16) & 0xFF) as u8,
                ((address >> 8) & 0xFF) as u8,
                (address & 0xFF) as u8,
                0,
            ],
            4,
        ),
        AddressMode::FourByte => (
            [
                command_4b,
                ((address >> 24) & 0xFF) as u8,
                ((address >> 16) & 0xFF) as u8,
                ((address >> 8) & 0xFF) as u8,
                (address & 0xFF) as u8,
            ],
            5,
        ),
    }
}

const fn crosses_boundary(address: u32, len: usize, boundary: u32) -> bool {
    if len == 0 {
        return false;
    }

    let last = address + (len as u32) - 1;
    (address / boundary) != (last / boundary)
}

const fn device_info_from_jedec<SpiError, PinError>(
    jedec_id: JedecId,
) -> Result<DeviceInfo, Error<SpiError, PinError>> {
    let capacity_bytes = match jedec_id.capacity_code {
        0x11 => 128 * 1024,
        0x12 => 256 * 1024,
        0x13 => 512 * 1024,
        0x14 => 1 * 1024 * 1024,
        0x15 => 2 * 1024 * 1024,
        0x16 => 4 * 1024 * 1024,
        0x17 => 8 * 1024 * 1024,
        0x18 => 16 * 1024 * 1024,
        0x19 => 32 * 1024 * 1024,
        0x20 => 64 * 1024 * 1024,
        other => return Err(Error::UnsupportedCapacityCode(other)),
    };

    let address_mode = if capacity_bytes > 16 * 1024 * 1024 {
        AddressMode::FourByte
    } else {
        AddressMode::ThreeByte
    };

    Ok(DeviceInfo {
        jedec_id,
        manufacturer: Manufacturer::from_jedec(jedec_id.manufacturer_code),
        address_mode,
        capacity_bytes,
        page_count: capacity_bytes / PAGE_SIZE,
        sector_count: capacity_bytes / SECTOR_SIZE,
        block_count: capacity_bytes / BLOCK_SIZE,
    })
}
