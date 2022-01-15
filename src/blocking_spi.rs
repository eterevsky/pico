use embedded_hal::spi::FullDuplex;
use embedded_time::{fixed_point::FixedPoint as _, rate::Extensions as _};
use rp2040_hal::{pac, spi};

pub struct SpiBlocking<D: spi::SpiDevice> {
    hal_spi: spi::Spi<spi::Enabled, D, 8>,
    dummy_data: u8,
}

impl<D: spi::SpiDevice> SpiBlocking<D> {
    pub fn new(
        resets: &mut pac::RESETS,
        spi_device: D,
    ) -> Self {
        let hal_spi = spi::Spi::<_, _, 8>::new(spi_device).init(
            resets,
            125_000_000u32.Hz(),
            12_000_000u32.Hz(),
            &embedded_hal::spi::MODE_0,
        );

        SpiBlocking {
            hal_spi,
            dummy_data: 0
        }
    }

    pub fn set_dummy_data(&mut self, byte: u8) {
        self.dummy_data = byte;
    }

    pub fn write_byte(&mut self, byte: u8) {
        // The only error that this method is returning is when the FIFO
        // is not writable.
        while self.hal_spi.send(byte).is_err() {}
    }

    pub fn write(&mut self, data: &[u8]) {
        for byte in data.iter() {
            self.write_byte(*byte);
        }
    }

    pub fn read_byte(&mut self) -> u8 {
        if let Ok(res) = self.hal_spi.read() {
            return res
        }
        self.write_byte(self.dummy_data);
        loop {
            if let Ok(res) = self.hal_spi.read() {
                return res;
            }
        }
    }
}