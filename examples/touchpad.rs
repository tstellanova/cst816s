#![no_std]
#![no_main]

extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics

use nrf52832_hal as p_hal;
use p_hal::gpio::{GpioExt, Level};
use p_hal::nrf52832_pac as pac;
use p_hal::{delay::Delay, rng::RngExt, spim, twim};

use cortex_m_rt as rt;
use cst816s::{
    TouchEvent, CST816S, GESTURE_LONG_PRESS, GESTURE_SINGLE_CLICK, GESTURE_SLIDE_DOWN,
    GESTURE_SLIDE_LEFT, GESTURE_SLIDE_RIGHT, GESTURE_SLIDE_UP,
};
use embedded_graphics::pixelcolor::{raw::RawU16, Rgb565};
use embedded_graphics::{prelude::*, primitives::*, style::*};
use embedded_hal::digital::v2::OutputPin;
use rt::entry;
use st7789::Orientation;

use embedded_hal::blocking::delay::{DelayMs, DelayUs};
use nrf52832_hal::prelude::ClocksExt;

pub type HalSpimError = p_hal::spim::Error;

pub type Spim0PortType = p_hal::spim::Spim<pac::SPIM0>;
pub type DisplaySckPinType = p_hal::gpio::p0::P0_18<p_hal::gpio::Output<p_hal::gpio::PushPull>>;
pub type DisplayMosiPinType = p_hal::gpio::p0::P0_26<p_hal::gpio::Output<p_hal::gpio::PushPull>>;

const SCREEN_WIDTH: i32 = 240;
const SCREEN_HEIGHT: i32 = 240;
const HALF_SCREEN_WIDTH: i32 = SCREEN_WIDTH / 2;
const MIN_SCREEN_DIM: i32 = SCREEN_HEIGHT;
const SCREEN_RADIUS: u32 = (MIN_SCREEN_DIM / 2) as u32;

///
/// This example was written and tested for the PineTime smart watch
///
#[entry]
fn main() -> ! {
    let cp = pac::CorePeripherals::take().unwrap();
    let mut delay_source = Delay::new(cp.SYST);

    // PineTime has a 32 MHz HSE (HFXO) and a 32.768 kHz LSE (LFXO)
    // Optimize clock config
    let dp = pac::Peripherals::take().unwrap();
    let _clockit = dp.CLOCK.constrain().enable_ext_hfosc();

    let port0 = dp.P0.split();

    // random number generator peripheral
    let mut rng = dp.RNG.constrain();

    // vibration motor output: drive low to activate motor
    let mut vibe = port0.p0_16.into_push_pull_output(Level::High).degrade();
    pulse_vibe(&mut vibe, &mut delay_source, 10);

    // internal i2c0 bus devices: BMA421 (accel), HRS3300 (hrs), CST816S (TouchPad)
    // BMA421-INT:  P0.08
    // TP-INT: P0.28
    let i2c0_pins = twim::Pins {
        scl: port0.p0_07.into_floating_input().degrade(),
        sda: port0.p0_06.into_floating_input().degrade(),
    };
    let i2c_port = twim::Twim::new(dp.TWIM1, i2c0_pins, twim::Frequency::K400);
    // let i2c_bus0 = shared_bus::CortexMBusManager::new(i2c_port);

    delay_source.delay_ms(1u8);

    let spim0_pins = spim::Pins {
        sck: port0.p0_02.into_push_pull_output(Level::Low).degrade(),
        miso: None,
        mosi: Some(port0.p0_03.into_push_pull_output(Level::Low).degrade()),
    };

    // create SPIM0 interface, 8 Mbps, use 122 as "over read character"
    let spim0 = spim::Spim::new(dp.SPIM0, spim0_pins, spim::Frequency::M8, spim::MODE_3, 122);
    let spi_bus0 = shared_bus::CortexMBusManager::new(spim0);

    // backlight control pin for display: always on
    let mut _backlight = port0.p0_22.into_push_pull_output(Level::Low);
    // SPI chip select (CSN) for the display.
    let display_csn = port0.p0_25.into_push_pull_output(Level::High);
    // data/clock switch pin for display
    let display_dc = port0.p0_18.into_push_pull_output(Level::Low);
    // reset pin for display
    let display_rst = port0.p0_26.into_push_pull_output(Level::Low);

    // create display driver
    let mut display = st7789::new_display_driver(
        spi_bus0.acquire(),
        display_csn,
        display_dc,
        display_rst,
        SCREEN_WIDTH as u16,
        SCREEN_HEIGHT as u16,
    );
    display.init(&mut delay_source).unwrap();
    display.set_orientation(&Orientation::Portrait).unwrap();

    draw_background(&mut display);

    // setup touchpad external interrupt pin: P0.28/AIN4 (TP_INT)
    let touch_int = port0.p0_28.into_pullup_input().degrade();
    // setup touchpad reset pin: P0.10/NFC2 (TP_RESET)
    let touch_rst = port0.p0_10.into_push_pull_output(Level::High).degrade();

    let mut touchpad = CST816S::new(i2c_port, touch_int, touch_rst);
    touchpad.setup(&mut delay_source).unwrap();

    let mut refresh_count = 0;
    loop {
        let rand_val = rng.random_u16();
        let rand_color = Rgb565::from(RawU16::new(rand_val));

        if let Some(evt) = touchpad.read_one_touch_event(true) {
            refresh_count += 1;
            if refresh_count > 100 {
                draw_background(&mut display);
                refresh_count = 0;
            }

            draw_marker(&mut display, &evt, rand_color);
            let vibe_time = match evt.gesture {
                cst816s::GESTURE_LONG_PRESS => {
                    refresh_count = 100;
                    50_000
                }
                cst816s::GESTURE_SINGLE_CLICK => 5_000,
                _ => 0,
            };

            pulse_vibe(&mut vibe, &mut delay_source, vibe_time);
        } else {
            delay_source.delay_us(1u32);
        }
    }
}

fn draw_background(display: &mut impl DrawTarget<Rgb565>) {
    let clear_bg = Rectangle::new(Point::new(0, 0), Point::new(SCREEN_WIDTH, SCREEN_HEIGHT))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK));
    clear_bg.draw(display).map_err(|_| ()).unwrap();

    let center_circle = Circle::new(
        Point::new(HALF_SCREEN_WIDTH, SCREEN_HEIGHT / 2),
        SCREEN_RADIUS,
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 4));
    center_circle.draw(display).map_err(|_| ()).unwrap();
}

const SWIPE_LENGTH: i32 = 20;
const SWIPE_WIDTH: i32 = 2;

/// Draw an indicator of the kind of gesture we detected
fn draw_marker(display: &mut impl DrawTarget<Rgb565>, event: &TouchEvent, color: Rgb565) {
    let x_pos = event.x;
    let y_pos = event.y;

    match event.gesture {
        GESTURE_SLIDE_LEFT | GESTURE_SLIDE_RIGHT => {
            Rectangle::new(
                Point::new(x_pos - SWIPE_LENGTH, y_pos - SWIPE_WIDTH),
                Point::new(x_pos + SWIPE_LENGTH, y_pos + SWIPE_WIDTH),
            )
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display)
            .map_err(|_| ())
            .unwrap();
        }
        GESTURE_SLIDE_UP | GESTURE_SLIDE_DOWN => {
            Rectangle::new(
                Point::new(x_pos - SWIPE_WIDTH, y_pos - SWIPE_LENGTH),
                Point::new(x_pos + SWIPE_WIDTH, y_pos + SWIPE_LENGTH),
            )
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display)
            .map_err(|_| ())
            .unwrap();
        }
        GESTURE_SINGLE_CLICK => Circle::new(Point::new(x_pos, y_pos), 20)
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display)
            .map_err(|_| ())
            .unwrap(),
        GESTURE_LONG_PRESS => {
            Circle::new(Point::new(x_pos, y_pos), 40)
                .into_styled(PrimitiveStyle::with_stroke(color, 4))
                .draw(display)
                .map_err(|_| ())
                .unwrap();
        }
        _ => {}
    }
}

/// Pulse the vibration motor briefly
fn pulse_vibe(vibe: &mut impl OutputPin, delay_source: &mut impl DelayUs<u32>, micros: u32) {
    if micros > 0 {
        let _ = vibe.set_low();
        delay_source.delay_us(micros);
        let _ = vibe.set_high();
    }
}
