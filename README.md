# spi-flash-for-embassy

Embassy 환경에서 일반적인 SPI NOR flash를 제어하기 위한 async Rust 라이브러리입니다.

원본 C 프로젝트 `~/Desktop/spif`의 핵심 기능을 Rust 스타일로 옮겼습니다.

제공 기능
- JEDEC ID 읽기 및 용량/주소 모드 자동 판별
- chip / sector / block erase
- address / page / sector / block read
- address / page / sector / block write
- page 경계 자동 분할 write
- `embedded-hal` 1.0 + `embedded-hal-async` 1.0 기반

기하 정보
- page: 256 bytes
- sector: 4 KiB
- block: 64 KiB
- 256 Mbit 이상은 4-byte address command 사용

기본 사용 예시
```rust
use spi_flash_for_embassy::SpiFlash;

// spi: embedded_hal_async::spi::SpiBus 구현체
// cs:  embedded_hal::digital::OutputPin 구현체
// delay: embedded_hal_async::delay::DelayNs 구현체

let mut flash = SpiFlash::new(spi, cs, delay);
let info = flash.initialize().await?;

flash.erase_sector(0).await?;
flash.write_address(0, b"hello flash").await?;

let mut buf = [0u8; 11];
flash.read_address(0, &mut buf).await?;
assert_eq!(&buf, b"hello flash");
```

Embassy STM32에서의 연결 방향
- `embassy_stm32::spi::Spi`를 생성
- CS는 일반 GPIO output으로 준비
- `embassy_time::Delay` 또는 동등한 delay 구현체 사용
- 필요하면 `embedded-hal-bus`의 exclusive/shared device 레이어 대신, 이 crate에는 SPI bus + CS pin을 직접 넘기면 됩니다.

원본 C API와의 대응
- `SPIF_Init` -> `initialize`
- `SPIF_EraseChip` -> `erase_chip`
- `SPIF_EraseSector` -> `erase_sector`
- `SPIF_EraseBlock` -> `erase_block`
- `SPIF_WriteAddress` -> `write_address`
- `SPIF_WritePage` -> `write_page`
- `SPIF_WriteSector` -> `write_sector`
- `SPIF_WriteBlock` -> `write_block`
- `SPIF_ReadAddress` -> `read_address`
- `SPIF_ReadPage` -> `read_page`
- `SPIF_ReadSector` -> `read_sector`
- `SPIF_ReadBlock` -> `read_block`

주의사항
- NOR flash의 program 동작은 erase된 영역(보통 0xFF) 기준으로 사용하는 것이 안전합니다.
- 이 라이브러리는 표준적인 SPI NOR command set을 기준으로 합니다.
- 특수 보호 비트, quad mode, suspend/resume, SFDP parsing 등은 아직 포함하지 않았습니다.

검증
- `cargo test`
