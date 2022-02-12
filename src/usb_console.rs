use core::cell::RefCell;
use core::fmt::Write as _;
use rp2040_hal as hal;
use rp2040_hal::{pac::interrupt, usb::UsbBus};
use usb_device::{
    bus::UsbBusAllocator,
    device::{UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_serial::{SerialPort, UsbError};

static mut USB_BUS: Option<UsbBusAllocator<UsbBus>> = None;

struct UsbManager {
    device: UsbDevice<'static, UsbBus>,
    serial: SerialPort<'static, UsbBus>,
}

impl UsbManager {
    fn new(usb_bus: &'static UsbBusAllocator<UsbBus>) -> Self {
        let serial = usbd_serial::SerialPort::new(usb_bus);

        let device = UsbDeviceBuilder::new(usb_bus, UsbVidPid(0x2E8A, 0x000a))
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
    unsafe{
        USB_BUS = Some(UsbBusAllocator::new(UsbBus::new(
            usbctrl_regs,
            usbctrl_dpram,
            usb_clock,
            /*force_vbus_detect_bit*/ true,
            resets,
        )));
    }

    {
        let manager = UsbManager::new(unsafe { USB_BUS.as_ref().unwrap() } );
        borrow_manager(|opt_manager| {
            opt_manager.insert(manager);
        })
    }
}

#[derive(Clone, Copy)]
pub struct UsbConsole;

impl UsbConsole {
    pub fn ready(&self) -> bool {
        borrow_manager(|manager| {
            if let Some(m) = manager {
                m.ready()
            } else {
                false
            }
        })
    }

    // Write bytes to the USB serial in UsbManager.
    // Returns the number of bytes that were actually written (added to the output buffer).
    fn write(&self, bytes: &[u8]) -> usbd_serial::Result<usize> {
        borrow_manager(|manager| {
            if let Some(m) = manager {
                m.serial.write(bytes)
            } else {
                Err(usbd_serial::UsbError::InvalidState)
            }
        })
    }
}

impl core::fmt::Write for UsbConsole {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if !self.ready() {
            return Result::Err(core::fmt::Error);
        }

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

    fn flush(&self) {}
}

static USB_CONSOLE: UsbConsole = UsbConsole;

pub fn get_console() -> &'static UsbConsole {
    &USB_CONSOLE
}
