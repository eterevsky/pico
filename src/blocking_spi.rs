use core::ops::Deref;
use rp2040_hal::pac;
use log::info;

pub trait Resettable {
    fn reset(&self, resets: &mut pac::RESETS);
    fn unreset(&self, resets: &mut pac::RESETS);
}

impl Resettable for pac::SPI0 {
    fn reset(&self, resets: &mut pac::RESETS) {
        resets.reset.modify(|_, w| w.spi0().set_bit());
    }

    fn unreset(&self, resets: &mut pac::RESETS) {
        resets.reset.modify(|_, w| w.spi0().clear_bit());
        while resets.reset_done.read().spi0().bit_is_clear() {}
    }
}

impl Resettable for pac::SPI1 {
    fn reset(&self, resets: &mut pac::RESETS) {
        resets.reset.modify(|_, w| w.spi1().set_bit());
    }

    fn unreset(&self, resets: &mut pac::RESETS) {
        resets.reset.modify(|_, w| w.spi1().clear_bit());
        while resets.reset_done.read().spi1().bit_is_clear() {}
    }
}

pub trait SpiDevice: Deref<Target = pac::spi0::RegisterBlock> + Resettable {}

impl SpiDevice for pac::SPI0 {}
impl SpiDevice for pac::SPI1 {}

#[derive(Clone, Copy)]
pub enum Mode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

impl Mode {
    fn cpol(self) -> bool {
        match self {
            Mode::Mode0 => false,
            Mode::Mode1 => false,
            Mode::Mode2 => true,
            Mode::Mode3 => true,
        }
    }

    fn cpha(self) -> bool {
        match self {
            Mode::Mode0 => false,
            Mode::Mode1 => true,
            Mode::Mode2 => false,
            Mode::Mode3 => true,
        }
    }
}

pub struct Spi<D: SpiDevice> {
    device: D,
    dummy_data: u8,
}

impl<D: SpiDevice> Spi<D> {
    pub fn new(device: D) -> Self {
        Spi {
            device,
            dummy_data: 0,
        }
    }

    pub fn init(&mut self, resets: &mut pac::RESETS, baudrate: u32) -> u32 {
        info!("device.reset");
        self.device.reset(resets);
        info!("device.unreset");
        self.device.unreset(resets);

        info!("set_baudrate");
        let actual_baudrate = self.set_baudrate(baudrate);
        info!("actual baudrate: {actual_baudrate}");

        // Use internal enum for format.
        self.set_format(8, Mode::Mode0);

        // Enable DREQ signals -- harmless if DMA is not listening
        self.device
            .sspdmacr
            .modify(|_, w| w.txdmae().set_bit().rxdmae().set_bit());

        // Enable SPI
        self.device.sspcr1.modify(|_, w| w.sse().set_bit());

        actual_baudrate
    }

    pub fn set_dummy_data(&mut self, byte: u8) {
        self.dummy_data = byte;
    }

    fn set_baudrate(&mut self, baudrate: u32) -> u32 {
        // Default peripheral clock frequency (same as system clock).
        let freq: u32 = 125_000_000;

        let prescale = if 3 * 256 * baudrate as u64 > freq as u64 {
            2
        } else {
            2 * (freq / (512 * baudrate))
        };

        let postdiv = (freq / (baudrate * prescale)) as u8;
        let prescale = prescale as u8;

        self.device
            .sspcpsr
            .write(|w| unsafe { w.cpsdvsr().bits(prescale) });
        self.device
            .sspcr0
            .modify(|_, w| unsafe { w.scr().bits(postdiv) });

        freq as u32 / ((prescale as u32) * (1 + postdiv as u32))
    }

    fn set_format(&mut self, data_bits: u8, mode: Mode) {
        self.device.sspcr0.modify(|_, w| unsafe {
            w.dss()
                .bits(data_bits - 1)
                .spo()
                .bit(mode.cpol())
                .sph()
                .bit(mode.cpha())
        });
    }

    fn _is_writable(&self) -> bool {
        self.device.sspsr.read().tnf().bit_is_set()
    }

    fn _is_readable(&self) -> bool {
        self.device.sspsr.read().rne().bit_is_set()
    }

    // Internal. Doesn't check that the device is writable.
    fn _write(&self, data: u8) {
        self.device
            .sspdr
            .write(|w| unsafe { w.data().bits(data as u16) });
    }

    // Internal. Doesn't check that the device is writable.
    fn _read(&self) -> u8 {
        self.device.sspdr.read().data().bits() as u8
    }

    pub fn write_byte(&mut self, byte: u8) {
        while !self._is_writable() {}
        self._write(byte);
    }

    pub fn write(&mut self, data: &[u8]) {
        for byte in data.iter() {
            self.write_byte(*byte);
        }
    }

    pub fn read_byte(&mut self) -> u8 {
        self.write_byte(self.dummy_data);
        while !self._is_readable() {}
        self._read()
    }
}
