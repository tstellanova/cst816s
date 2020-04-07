#![no_std]


use core::fmt::Debug;


use embedded_hal as hal;
use hal::blocking::i2c::{WriteRead}; //, , Read};
use hal::blocking::delay::{DelayUs};
use arrayvec::ArrayVec;
// use hal::digital::v2::{InputPin, StatefulOutputPin};


/// Errors in this crate
#[derive(Debug)]
pub enum Error<CommE, PinE> {
    Comm(CommE),
    Pin(PinE),

    GenericError,

}

pub struct CST816S<I2C, PINT, RST> {
    i2c: I2C,
    pin_int: PINT,
    pin_rst:  RST,
    blob_buf: [u8; BLOB_BUF_LEN],
}

#[derive(Debug)]
pub struct TouchEvent {
    pub x: i32,
    pub y: i32,
    /// the gesture that this touch is part of
    pub gesture: u8,
    /// 0 down, 1 lift, 2 contact
    pub action: u8,
    /// identifies the finger that touched (0-9)
    pub finger_id: u8,
    /// pressure level of touch
    pub pressure: u8,
    /// the surface area of the touch
    pub area: u8,
}

impl<I2C, PINT, RST, CommE, PinE> CST816S<I2C, PINT, RST>
where
    I2C: hal::blocking::i2c::Write<Error = CommE>
    + hal::blocking::i2c::Read<Error = CommE>
    + hal::blocking::i2c::WriteRead<Error = CommE>,
    PINT: hal::digital::v2::InputPin,
    RST: hal::digital::v2::StatefulOutputPin<Error = PinE>,
{

    pub fn new(port: I2C, interrupt_pin: PINT, reset_pin: RST) -> Self {
        Self {
            i2c: port,
            pin_int: interrupt_pin,
            pin_rst: reset_pin,
            blob_buf: [0u8; BLOB_BUF_LEN],
        }
    }

    /// setup the driver to communicate with the device
    pub fn setup(&mut self, delay_source: &mut impl DelayUs<u32>)
        -> Result<(), Error<CommE, PinE>>
    {
        // reset the chip
        self.pin_rst.set_low().map_err(Error::Pin)?;
        delay_source.delay_us(20_000);
        self.pin_rst.set_high().map_err(Error::Pin)?;
        delay_source.delay_us(400_000);

        //TODO setup interrupt on pin_int

        Ok(())
    }

    /// Read enough registers to fill our read buf
    pub fn read_registers(&mut self)
        -> Result<(), Error<CommE, PinE>> {

        // //TODO does write_read work for this device? or do we need separate writes and reads?
        // self.i2c.write_read(Self::DEFAULT_I2C_ADDRESS,
        //                     &[Self::REG_FIRST],
        //                     self.blob_buf.as_mut()).map_err(Error::Comm)?;

        self.i2c.write(Self::DEFAULT_I2C_ADDRESS, &[Self::REG_FIRST]).map_err(Error::Comm)?;
        self.i2c.read(Self::DEFAULT_I2C_ADDRESS, self.blob_buf.as_mut()).map_err(Error::Comm)?;

        Ok(())
    }



    ///
    /// Translate raw register data into touch events
    ///
    fn touch_event_from_data(buf: &[u8]) -> Option<TouchEvent> {
        let mut touch = TouchEvent {
            x: 0,
            y: 0,
            gesture: 0,
            action: 0,
            finger_id: 0,
            pressure: 0,
            area: 0
        };

        // two of the registers mix 4 bits of position with other values
        // four high bits of X and 2 bits of Action:
        let touch_x_h_and_action = buf[Self::TOUCH_X_H_AND_ACTION_OFF];
        // four high bits of Y and 4 bits of Finger:
        let touch_y_h_and_finger = buf[Self::TOUCH_Y_H_AND_FINGER_OFF];

        // X and Y position are both 12 bits, in pixels from top left corner?
        touch.x = (buf[Self::TOUCH_X_L_OFF] as i32) | (((touch_x_h_and_action & 0x0F) as i32) << 8);
        touch.y = (buf[Self::TOUCH_Y_L_OFF] as i32) | (((touch_y_h_and_finger & 0x0F) as i32) << 8);

        // action of touch (0 = down, 1 = up, 2 = contact)
        touch.action = (touch_x_h_and_action >> 6) as u8;
        touch.finger_id  = (touch_y_h_and_finger >> 4) as u8;

        //  Compute touch pressure and area
        touch.pressure = buf[Self::TOUCH_PRESURE_OFF ];
        touch.area = buf[Self::TOUCH_AREA_OFF ] >> 4;

        Some(touch)
    }

    /// The main method for getting the current set of touch events
    /// Reads events into the event buffer provided
    pub fn read_one_touch_event(&mut self, ) -> Option<TouchEvent> {

        if self.read_registers().is_ok() {
            let gesture_id = self.blob_buf[Self::GESTURE_ID_OFF]; //TODO report gestures
            let num_points = (self.blob_buf[Self::NUM_POINTS_OFF] & 0x0F) as usize;
            for i in 0..num_points {
                let evt_start: usize = (i * Self::RAW_TOUCH_EVENT_LEN) + Self::GESTURE_HEADER_LEN;
                if let Some(mut evt) = Self::touch_event_from_data(
                    self.blob_buf[evt_start..evt_start + Self::RAW_TOUCH_EVENT_LEN].as_ref())
                {
                    evt.gesture = gesture_id;
                    //TODO we only ever appear to get one event on the PineTime: handle more?
                    return Some(evt)
                }
            }
        }

       None
    }

    const DEFAULT_I2C_ADDRESS: u8 = 0x15;

    pub const GESTURE_HEADER_LEN: usize = 3;
    /// Number of bytes for a single touch event
    pub const RAW_TOUCH_EVENT_LEN: usize = 6;

    /// The first register on the device
    const REG_FIRST:u8 = 	0x00;

    /// Header bytes (first three of every register block read)
    // const RESERVED_0_OFF: usize = 0;
    const GESTURE_ID_OFF: usize = 1;
    const NUM_POINTS_OFF: usize = 2;

    /// These offsets are relative to the body start (after NUM_POINTS_OFF)
    /// offset of touch X position high bits and Action bits
    const TOUCH_X_H_AND_ACTION_OFF: usize = 0;
    /// offset of touch X position low bits
    const TOUCH_X_L_OFF: usize = 1;
    /// offset of touch Y position high bits and Finger bits
    const TOUCH_Y_H_AND_FINGER_OFF: usize = 2;
    /// offset of touch Y position low bits
    const TOUCH_Y_L_OFF: usize = 3;
    const TOUCH_PRESURE_OFF: usize = 4;
    const TOUCH_AREA_OFF: usize = 5;

}


/// In essence, max number of fingers
pub const MAX_TOUCH_CHANNELS: usize = 10;

const BLOB_BUF_LEN: usize = 63; // (MAX_TOUCH_CHANNELS + CST816S::RAW_TOUCH_EVENT_LEN) + CST816S::GESTURE_HEADER_LEN;



#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
