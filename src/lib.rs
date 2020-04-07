#![no_std]


use core::fmt::Debug;
use core::convert::TryInto;
use core::convert::TryFrom;
use embedded_hal::blocking::i2c::{WriteRead, Write};
use embedded_hal::blocking::delay::DelayMs;



pub struct CST816S<I2C> {
    i2c: I2C,
}


impl<I2C> CST816S<I2C> {

    pub fn new(port: I2C) -> Self {
        Self {
            i2c: port
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
