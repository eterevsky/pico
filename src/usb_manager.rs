use core::cell::RefCell;
use cortex_m::interrupt::{CriticalSection, Mutex};
use rp2040_hal::usb::UsbBus;
use usb_device::{
    bus::UsbBusAllocator,
    device::{UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_serial::{SerialPort, UsbError};


pub struct UsbManager {
    device: UsbDevice<'static, UsbBus>,
    serial: SerialPort<'static, UsbBus>,
}

impl UsbManager {
    pub fn new(usb_bus: &'static UsbBusAllocator<UsbBus>,
) -> Self {
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

    pub unsafe fn interrupt(&mut self) {
        if self.device.poll(&mut [&mut self.serial]) {}
    }

    pub fn ready(&self) -> bool {
        self.serial.dtr() && self.serial.rts()
    }
}

impl core::fmt::Write for UsbManager {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if !self.ready() {
            return Result::Err(core::fmt::Error)
        }

        let mut bytes_to_send = s.as_bytes();

        while !bytes_to_send.is_empty() {
            match self.serial.write(bytes_to_send) {
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

static USB_MANAGER: cortex_m::interrupt::Mutex<RefCell<Option<UsbManager>>> = Mutex::new(RefCell::new(None));

// Execute a closure with &mut UsbManager. The closure will be executed in interrupt-free context
// and must not block.
fn borrow_manager<F, R>(f: F) -> R
where F: FnOnce(&mut Option<UsbManager>) -> R {
    cortex_m::interrupt::free(|cs| {
        let mut manager = USB_MANAGER.borrow(cs).borrow_mut();
        f(&mut *manager)
    })
}

pub struct UsbConsole;
