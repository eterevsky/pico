//! Writes to USB console via log.
#![no_std]
#![no_main]

use embedded_time::fixed_point::FixedPoint as _;
use log::info;
use rp2040_hal as hal;
use rp2040_hal::{clocks::Clock as _, pac, watchdog::Watchdog};

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

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

    let console = pico_usb_console::get_console();

    unsafe {
        log::set_logger_racy(console)
            .map(|()| log::set_max_level(log::LevelFilter::Info))
            .unwrap();
    }

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());
    let ms = pico_usb_console::wait_until_ready(&mut delay);
    info!("Hello (latency: {ms} ms)");

    let mut i = 0;

    loop {
        delay.delay_ms(1000);
        i += 1;
        info!("{i}");
    }
}
