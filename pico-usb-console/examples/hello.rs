//! Writes to USB console.
#![no_std]
#![no_main]

use core::fmt::Write as _;
use core::panic::PanicInfo;
use embedded_time::fixed_point::FixedPoint as _;
use rp2040_hal as hal;
use rp2040_hal::{clocks::Clock as _, pac, watchdog::Watchdog};

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    let mut usb = *pico_usb_console::get_console();
    write!(&mut usb, "{}\n", panic_info).ok();
    loop {}
}

// External high-speed crystal on the pico board is 12Mhz
pub const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);

    let clocks = hal::clocks::init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    pico_usb_console::init_usb_manager(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        &mut pac.RESETS,
    );

    let mut usb = *pico_usb_console::get_console();

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());

    let latency = pico_usb_console::wait_until_ready(&mut delay);
    writeln!(usb, "Hello (latency: {} ms)", latency).unwrap();

    let mut i = 0;

    loop {
        delay.delay_ms(1000);
        i += 1;
        writeln!(usb, "{}", i).unwrap();
    }
}
