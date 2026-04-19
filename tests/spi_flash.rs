use core::convert::Infallible;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use embedded_hal::digital::{ErrorType as DigitalErrorType, OutputPin};
use embedded_hal::spi::{ErrorKind, ErrorType as SpiErrorType};
use embedded_hal_async::{delay::DelayNs, spi::SpiBus};
use futures::executor::block_on;
use spi_flash_for_embassy::{AddressMode, Error, Manufacturer, SpiFlash};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Operation {
    CsLow,
    CsHigh,
    SpiWrite(Vec<u8>),
    SpiRead(Vec<u8>),
    DelayMs(u32),
}

#[derive(Clone)]
struct MockSpi {
    expected: Rc<RefCell<VecDeque<Operation>>>,
}

impl MockSpi {
    fn new(expected: Rc<RefCell<VecDeque<Operation>>>) -> Self {
        Self { expected }
    }
}

impl SpiErrorType for MockSpi {
    type Error = ErrorKind;
}

impl SpiBus for MockSpi {
    async fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        match self.expected.borrow_mut().pop_front() {
            Some(Operation::SpiRead(data)) => {
                assert_eq!(data.len(), words.len(), "read length mismatch");
                words.copy_from_slice(&data);
                Ok(())
            }
            other => panic!("unexpected SPI read operation: {other:?}"),
        }
    }

    async fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        match self.expected.borrow_mut().pop_front() {
            Some(Operation::SpiWrite(expected)) => {
                assert_eq!(expected, words, "write bytes mismatch");
                Ok(())
            }
            other => panic!("unexpected SPI write operation: {other:?}"),
        }
    }

    async fn transfer(&mut self, _read: &mut [u8], _write: &[u8]) -> Result<(), Self::Error> {
        unimplemented!("transfer is not used by this driver");
    }

    async fn transfer_in_place(&mut self, _words: &mut [u8]) -> Result<(), Self::Error> {
        unimplemented!("transfer_in_place is not used by this driver");
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone)]
struct MockCs {
    expected: Rc<RefCell<VecDeque<Operation>>>,
}

impl MockCs {
    fn new(expected: Rc<RefCell<VecDeque<Operation>>>) -> Self {
        Self { expected }
    }
}

impl DigitalErrorType for MockCs {
    type Error = Infallible;
}

impl OutputPin for MockCs {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        match self.expected.borrow_mut().pop_front() {
            Some(Operation::CsLow) => Ok(()),
            other => panic!("unexpected CS low operation: {other:?}"),
        }
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        match self.expected.borrow_mut().pop_front() {
            Some(Operation::CsHigh) => Ok(()),
            other => panic!("unexpected CS high operation: {other:?}"),
        }
    }
}

#[derive(Clone)]
struct MockDelay {
    expected: Rc<RefCell<VecDeque<Operation>>>,
}

impl MockDelay {
    fn new(expected: Rc<RefCell<VecDeque<Operation>>>) -> Self {
        Self { expected }
    }
}

impl DelayNs for MockDelay {
    async fn delay_ns(&mut self, ns: u32) {
        if ns % 1_000_000 != 0 {
            panic!("unexpected non-ms delay: {ns}");
        }

        match self.expected.borrow_mut().pop_front() {
            Some(Operation::DelayMs(expected_ms)) => {
                assert_eq!(expected_ms * 1_000_000, ns, "delay mismatch");
            }
            other => panic!("unexpected delay operation: {other:?}"),
        }
    }
}

fn shared_ops(ops: Vec<Operation>) -> Rc<RefCell<VecDeque<Operation>>> {
    Rc::new(RefCell::new(VecDeque::from(ops)))
}

#[test]
fn initialize_reads_jedec_and_geometry() {
    let ops = shared_ops(vec![
        Operation::CsHigh,
        Operation::DelayMs(20),
        Operation::CsLow,
        Operation::SpiWrite(vec![0x04]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x9F]),
        Operation::SpiRead(vec![0xEF, 0x40, 0x17]),
        Operation::CsHigh,
    ]);

    let spi = MockSpi::new(ops.clone());
    let cs = MockCs::new(ops.clone());
    let delay = MockDelay::new(ops.clone());
    let mut flash = SpiFlash::new(spi, cs, delay);

    let info = block_on(flash.initialize()).expect("initialize should succeed");

    assert_eq!(info.manufacturer, Manufacturer::Winbond);
    assert_eq!(info.address_mode, AddressMode::ThreeByte);
    assert_eq!(info.capacity_bytes, 8 * 1024 * 1024);
    assert!(
        ops.borrow().is_empty(),
        "all mock operations should be consumed"
    );
}

#[test]
fn write_address_splits_page_programs_across_boundaries() {
    let ops = shared_ops(vec![
        Operation::CsHigh,
        Operation::DelayMs(20),
        Operation::CsLow,
        Operation::SpiWrite(vec![0x04]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x9F]),
        Operation::SpiRead(vec![0xEF, 0x40, 0x17]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x06]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x02, 0x00, 0x00, 0xFE]),
        Operation::SpiWrite(vec![0xAA, 0xBB]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x05]),
        Operation::SpiRead(vec![0x00]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x06]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x02, 0x00, 0x01, 0x00]),
        Operation::SpiWrite(vec![0xCC, 0xDD]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x05]),
        Operation::SpiRead(vec![0x00]),
        Operation::CsHigh,
    ]);

    let spi = MockSpi::new(ops.clone());
    let cs = MockCs::new(ops.clone());
    let delay = MockDelay::new(ops.clone());
    let mut flash = SpiFlash::new(spi, cs, delay);
    block_on(flash.initialize()).unwrap();

    block_on(flash.write_address(0xFE, &[0xAA, 0xBB, 0xCC, 0xDD])).unwrap();

    assert!(
        ops.borrow().is_empty(),
        "all mock operations should be consumed"
    );
}

#[test]
fn write_page_rejects_crossing_page_boundary() {
    let ops = shared_ops(vec![
        Operation::CsHigh,
        Operation::DelayMs(20),
        Operation::CsLow,
        Operation::SpiWrite(vec![0x04]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x9F]),
        Operation::SpiRead(vec![0xEF, 0x40, 0x17]),
        Operation::CsHigh,
    ]);
    let spi = MockSpi::new(ops.clone());
    let cs = MockCs::new(ops.clone());
    let delay = MockDelay::new(ops.clone());
    let mut flash = SpiFlash::new(spi, cs, delay);

    block_on(flash.initialize()).unwrap();
    let err = block_on(flash.write_page(0, 255, &[0xAA, 0xBB])).unwrap_err();
    assert_eq!(err, Error::OutOfBounds);
    assert!(
        ops.borrow().is_empty(),
        "all mock operations should be consumed"
    );
}

#[test]
fn initialize_uses_four_byte_addressing_for_large_parts() {
    let ops = shared_ops(vec![
        Operation::CsHigh,
        Operation::DelayMs(20),
        Operation::CsLow,
        Operation::SpiWrite(vec![0x04]),
        Operation::CsHigh,
        Operation::CsLow,
        Operation::SpiWrite(vec![0x9F]),
        Operation::SpiRead(vec![0xC8, 0x40, 0x19]),
        Operation::CsHigh,
    ]);

    let spi = MockSpi::new(ops.clone());
    let cs = MockCs::new(ops.clone());
    let delay = MockDelay::new(ops.clone());
    let mut flash = SpiFlash::new(spi, cs, delay);

    let info = block_on(flash.initialize()).unwrap();

    assert_eq!(info.manufacturer, Manufacturer::GigaDevice);
    assert_eq!(info.address_mode, AddressMode::FourByte);
    assert_eq!(info.capacity_bytes, 32 * 1024 * 1024);
    assert!(
        ops.borrow().is_empty(),
        "all mock operations should be consumed"
    );
}
