# Controlling Pimoroni Pico Wireless

Establishes SPI communication with ESP32 on the Pico Wireless, tries to connect to the WiFi.

## Compatibility

Tested with Pimoroni Pico LiPo + Pico WiFi.

and

* Windows 10/11
* macOS (elf2uf2-rs fails on binaries >128 KiB)

## Requirements

```
rustup target install thumbv6m-none-eabi
cargo install elf2uf2-rs
```

## Running

Connect Raspberry Pi Pico by USB while holding BOOTSEL (for Arduino Nano Connect ground 13th pin).

```
cargo run --release
```

(For some reason USB works only in release build.)