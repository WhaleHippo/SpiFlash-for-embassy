# spif upstream architecture analysis

1. Goal
- Source: `~/Desktop/spif`
- Target: `~/Desktop/SpiFlash-for-embassy`
- Objective: port the STM32 HAL + optional FreeRTOS SPI NOR flash helper into a reusable Rust + Embassy library.

2. Source structure
- `README.md`: high-level usage, mentions testing on Winbond W25Q64.
- `spif.h`: public API, geometry macros, JEDEC manufacturer/size enums, `SPIF_HandleTypeDef` runtime state.
- `spif.c`: full implementation including low-level SPI transactions, JEDEC detection, erase/program/read flows.
- `NimaLTD.I-CUBE-SPIF_conf.h`: compile-time switches for debug level, HAL vs HAL-DMA, RTOS flavor.

3. Public API surface in the C implementation
- Init / discovery:
  - `SPIF_Init`
- Erase:
  - `SPIF_EraseChip`
  - `SPIF_EraseSector`
  - `SPIF_EraseBlock`
- Write:
  - `SPIF_WriteAddress`
  - `SPIF_WritePage`
  - `SPIF_WriteSector`
  - `SPIF_WriteBlock`
- Read:
  - `SPIF_ReadAddress`
  - `SPIF_ReadPage`
  - `SPIF_ReadSector`
  - `SPIF_ReadBlock`

4. Core runtime model
- `SPIF_HandleTypeDef` stores:
  - SPI peripheral handle
  - CS GPIO + pin
  - JEDEC manufacturer / memory type / capacity code
  - init flag
  - software lock flag
  - derived geometry: page / sector / block counts
- The lock is a busy-wait software mutex intended for RTOS or concurrent callers.
- Rust port does not need this exact lock because `&mut self` already guarantees exclusive access in safe APIs.

5. Geometry and addressing assumptions
- Page size: 256 bytes
- Sector size: 4 KiB
- Block size: 64 KiB
- 3-byte commands for capacities below 256 Mbit
- 4-byte commands when block count >= 512, which corresponds to 256 Mbit and larger parts
- Capacity is inferred from JEDEC density code using a fixed lookup table.

6. Important low-level flows
- `SPIF_FindChip`
  - Sends `0x9F`
  - Reads manufacturer, memory type, size code
  - Derives block/sector/page counts
- `SPIF_WaitForWriting`
  - Polls status register 1 (`0x05`) until BUSY bit clears
  - Uses delay loop and timeout budget
- `SPIF_WriteFn`
  - Single-page page-program primitive
  - Clamps payload to page remainder
  - Sends write-enable (`0x06`)
  - Sends page-program command + address
  - Streams data payload
  - Polls until BUSY clears
- `SPIF_ReadFn`
  - Sends read command + address
  - Reads payload bytes
- Higher-level write helpers split across page boundaries as needed.

7. Porting recommendations for Rust + Embassy
- Use `embedded-hal` / `embedded-hal-async` traits instead of STM32 HAL handles.
- Store SPI, CS, and async delay objects inside a driver struct.
- Replace boolean return codes with a typed `Error` enum.
- Keep the same geometry model and JEDEC lookup table for compatibility.
- Preserve high-level convenience helpers:
  - address / page / sector / block read/write
  - sector / block / chip erase
- Prefer explicit bounds errors over silent truncation when an operation crosses a page/sector/block boundary.
- Keep page-splitting behavior for raw address writes.

8. MVP priorities
- JEDEC identification + geometry derivation
- Status polling / write-enable helpers
- Page program + address read
- Erase operations
- Page/sector/block convenience wrappers
- Unit tests with mock SPI/CS/delay objects

9. Pitfalls to preserve or improve
- Upstream silently truncates some operations; Rust API should make boundary violations explicit.
- Upstream relies on global timeouts via `HAL_GetTick`; Embassy port should convert this into delay-based polling with retry budgets.
- Upstream assumes standard SPI NOR command set; document that exotic chips may need extra support.

10. Files inspected
- `~/Desktop/spif/README.md`
- `~/Desktop/spif/spif.h`
- `~/Desktop/spif/spif.c`
- `~/Desktop/spif/NimaLTD.I-CUBE-SPIF_conf.h`
