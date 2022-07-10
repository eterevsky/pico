//! Blinks the LED on a Pico board and log to USB console
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[panic_handler]
fn panic(_panic_info: &PanicInfo) -> ! {
    loop {}
}

// External high-speed crystal on the pico board is 12Mhz
pub const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

#[cortex_m_rt::entry]
fn main() -> ! {
    let pac = rp2040_pac::Peripherals::take().unwrap();
    let pin_id = 25;

    pac.RESETS.reset.modify(|_, w| w.io_bank0().set_bit());
    pac.RESETS.reset.modify(|_, w| w.io_bank0().clear_bit());
    while pac.RESETS.reset_done.read().io_bank0().bit_is_clear() {}

    pac.IO_BANK0.gpio[pin_id].gpio_ctrl.modify(|_, w| w.funcsel().sio());

    pac.SIO.gpio_oe.write(|w| unsafe { w.bits(1 << pin_id) } );
    pac.SIO.gpio_out.write(|w| unsafe { w.bits(1 << pin_id) } );
    
    loop {}
}
