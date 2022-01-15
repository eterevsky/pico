//! Blinks the LED on a Pico board and log to USB console
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
//! USB console can be used with write!() macro and is also used for panic! error message.
#![no_std]
#![no_main]

use core::fmt::Write as _;
use core::panic::PanicInfo;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use embedded_time::{fixed_point::FixedPoint as _, rate::Extensions as _};
use log::info;
use rp2040_hal as hal;
use rp2040_hal::{
    clocks::Clock as _, gpio, pac, pac::interrupt, sio::Sio, spi::Spi, watchdog::Watchdog,
};
use usb_device;
use usb_device::bus::UsbBusAllocator;

mod blocking_spi;
mod pico_wireless;
mod usb_manager;

use crate::usb_manager::UsbManager;

#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

static mut USB_BUS: Option<UsbBusAllocator<hal::usb::UsbBus>> = None;
static mut USB_MANAGER: Option<UsbManager> = None;

#[allow(non_snake_case)]
#[interrupt]
unsafe fn USBCTRL_IRQ() {
    match USB_MANAGER.as_mut() {
        Some(manager) => manager.interrupt(),
        None => (),
    };
}

#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    if let Some(usb) = unsafe { USB_MANAGER.as_mut() } {
        writeln!(usb, "{}", panic_info).ok();
    }
    loop {}
}

struct UsbLogger;

impl log::Log for UsbLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if let Some(usb) = unsafe { USB_MANAGER.as_mut() } {
            writeln!(usb, "{}", record.args()).unwrap();
        }
    }

    fn flush(&self) {}
}

static LOGGER: UsbLogger = UsbLogger;

// External high-speed crystal on the pico board is 12Mhz
pub const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

const ESP_LED_R: u8 = 25;
const ESP_LED_G: u8 = 26;
const ESP_LED_B: u8 = 27;

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

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

    let usb = unsafe {
        USB_BUS = Some(UsbBusAllocator::new(hal::usb::UsbBus::new(
            pac.USBCTRL_REGS,
            pac.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut pac.RESETS,
        )));
        USB_MANAGER = Some(UsbManager::new(USB_BUS.as_ref().unwrap()));
        // Enable the USB interrupt
        pac::NVIC::unmask(hal::pac::Interrupt::USBCTRL_IRQ);
        USB_MANAGER.as_mut().unwrap()
    };

    unsafe {
        log::set_logger_racy(&LOGGER)
            .map(|()| log::set_max_level(log::LevelFilter::Info))
            .unwrap();
    }

    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().integer());
    let mut led_pin = pins.gpio25.into_push_pull_output();

    let button_a = pico_wireless::ButtonA::new(pins.gpio12);

    for i in 0..10 {
        info!("{}", i * 100);
        delay.delay_ms(100);
    }

    info!("Creating pins for SPI");

    let cs = pins.gpio7.into_push_pull_output();
    let gpio2 = pins.gpio2.into_push_pull_output();
    let resetn = pins.gpio11.into_push_pull_output();
    // let ack = pins.gpio10.into_bus_keep_input();
    let ack = pins.gpio10.into_pull_down_input();
    let _ = pins.gpio16.into_mode::<gpio::FunctionSpi>();
    let _ = pins.gpio18.into_mode::<gpio::FunctionSpi>();
    let _ = pins.gpio19.into_mode::<gpio::FunctionSpi>();

    info!("Creating ESP32 inteface");

    let mut esp32 = pico_wireless::Esp32::new(&mut pac.RESETS, pac.SPI0, cs, ack, gpio2, resetn, &mut delay);

    info!("Calling analog_write");

    esp32.analog_write(ESP_LED_G, 0).unwrap();

    loop {
        led_pin.set_high().unwrap();
        // esp32.analog_write(ESP_LED_R, 255).unwrap();
        // esp32.analog_write(ESP_LED_B, 0).unwrap();
        writeln!(usb, "On {}", button_a.pressed()).ok();
        delay.delay_ms(500);

        led_pin.set_low().unwrap();
        // esp32.analog_write(ESP_LED_R, 0).unwrap();
        // esp32.analog_write(ESP_LED_B, 255).unwrap();
        writeln!(usb, "Off {}", button_a.pressed()).ok();
        delay.delay_ms(500);
    }
}
