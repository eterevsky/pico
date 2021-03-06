#![no_std]

use core::cell::RefCell;
use core::fmt::Write as _;
use core::panic::PanicInfo;
use rp2040_hal as hal;
use rp2040_hal::{pac::interrupt, usb::UsbBus};
use usb_device::{
    bus::UsbBusAllocator,
    device::{UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_serial::{SerialPort, UsbError};

struct UsbManager {
    device: UsbDevice<'static, UsbBus>,
    serial: SerialPort<'static, UsbBus>,
}

impl UsbManager {
    fn new(alloc: &'static UsbBusAllocator<UsbBus>) -> Self {
        let serial = usbd_serial::SerialPort::new(alloc);

        let device = UsbDeviceBuilder::new(alloc, UsbVidPid(0x2E8A, 0x000a))
            .manufacturer("Raspberry Pi")
            .product("Pico")
            .serial_number("TEST")
            .device_class(2)
            .device_protocol(1)
            .build();

        UsbManager { device, serial }
    }

    unsafe fn interrupt(&mut self) {
        if self.device.poll(&mut [&mut self.serial]) {}
    }

    fn ready(&self) -> bool {
        self.serial.dtr() && self.serial.rts()
    }
}

static mut USB_BUS: Option<UsbBusAllocator<UsbBus>> = None;
static USB_MANAGER: cortex_m::interrupt::Mutex<RefCell<Option<UsbManager>>> =
    cortex_m::interrupt::Mutex::new(RefCell::new(None));

// Execute a closure with &mut UsbManager. The closure will be executed in interrupt-free context
// and must not block.
fn borrow_manager<F, R>(f: F) -> R
where
    F: FnOnce(&mut Option<UsbManager>) -> R,
{
    cortex_m::interrupt::free(|cs| {
        let mut manager = USB_MANAGER.borrow(cs).borrow_mut();
        f(&mut *manager)
    })
}

#[allow(non_snake_case)]
#[interrupt]
unsafe fn USBCTRL_IRQ() {
    borrow_manager(|manager| match manager {
        Some(m) => m.interrupt(),
        None => (),
    })
}

/// Initialize UsbBus and UsbManager. Will block until the USB connection is established.
pub fn init_usb_manager(
    usbctrl_regs: hal::pac::USBCTRL_REGS,
    usbctrl_dpram: hal::pac::USBCTRL_DPRAM,
    usb_clock: hal::clocks::UsbClock,
    resets: &mut hal::pac::RESETS,
) {
    let usb_bus = UsbBusAllocator::new(UsbBus::new(
        usbctrl_regs,
        usbctrl_dpram,
        usb_clock,
        true,
        resets,
    ));

    unsafe { USB_BUS = Some(usb_bus); }

    {
        let manager = UsbManager::new(unsafe { USB_BUS.as_ref().unwrap() } );
        borrow_manager(|opt_manager| {
            // Ignoring the returned reference.
            let _ = opt_manager.insert(manager);
        })
    }

    // Enable the USB interrupt
    unsafe { hal::pac::NVIC::unmask(hal::pac::Interrupt::USBCTRL_IRQ); }
}

pub fn usb_manager_initialized() -> bool {
    borrow_manager(|manager| manager.is_some())
}

fn usb_manager_ready() -> bool {
    borrow_manager(|manager| {
        if let Some(m) = manager {
            m.ready()
        } else {
            false
        }
    })
}

/// Waits until USB console is ready.
/// Returns the number of millisecond for which the function needed to block.
pub fn wait_until_ready(delay: &mut cortex_m::delay::Delay) -> u32 {
    let mut latency_ms = 0;
    while !usb_manager_ready() {
        delay.delay_ms(10);
        latency_ms += 10;
    }
    latency_ms
}

/// Waits until USB console is initialized.
/// Returns the number of millisecond for which the function needed to block.
pub fn wait_until_initialized(delay: &mut cortex_m::delay::Delay) -> u32 {
    let mut latency_ms = 0;
    while !usb_manager_initialized() {
        delay.delay_ms(10);
        latency_ms += 10;
    }
    latency_ms
}

#[derive(Clone, Copy)]
pub struct UsbConsole;

impl UsbConsole {
    pub fn ready(&self) -> bool { usb_manager_ready() }

    // Write bytes to the USB serial in UsbManager.
    // Returns the number of bytes that were actually written (added to the output buffer).
    fn write(&self, data: &[u8]) -> usbd_serial::Result<usize> {
        borrow_manager(|manager| {
            if let Some(m) = manager {
                m.serial.write(data)
            } else {
                Err(usbd_serial::UsbError::InvalidState)
            }
        })
    }
}

impl core::fmt::Write for UsbConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // if !self.ready() {
        //     return Result::Err(core::fmt::Error);
        // }

        let mut bytes_to_send = s.as_bytes();

        while !bytes_to_send.is_empty() {
            match self.write(bytes_to_send) {
                // Output buffer is full. Retry.
                Err(UsbError::WouldBlock) => (),

                // Shouldn't happen, but it's not like we can do much about it, unless there
                // is some panic handler not relying on the USB console.
                Err(e) => panic!("Error while writing to USB: {e:?}"),

                Ok(written_size) => {
                    // Keep only the tail that hasn't been sent yet.
                    bytes_to_send = &bytes_to_send[written_size..];
                }
            }
        }

        Ok(())
    }
}

impl log::Log for UsbConsole {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        let mut copy = *self;
        writeln!(&mut copy, "{}", record.args()).unwrap();
    }

    fn flush(&self) {
        loop {
            match borrow_manager(|manager| {
                if let Some(m) = manager {
                    m.serial.flush()
                } else {
                    Err(usbd_serial::UsbError::InvalidState)
                }
            }) {
                Ok(()) => return,

                // Output buffer hasn't been fully flushed. Retry.
                Err(UsbError::WouldBlock) => {},

                Err(e) => panic!("Error while flushing USB: {e:?}"),
            }
        }
    }
}

static USB_CONSOLE: UsbConsole = UsbConsole;

pub fn get_console() -> &'static UsbConsole {
    &USB_CONSOLE
}

#[cfg(feature = "panic")]
#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    let mut console = UsbConsole;
    write!(&mut console, "{}\n", panic_info).ok();
    loop {}
}

