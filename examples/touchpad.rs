#![no_std]
#![no_main]

// configure panic behavior:
#[cfg(not(debug_assertions))]
extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
#[cfg(debug_assertions)]
extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger

use nrf52832_hal as p_hal;
use p_hal::nrf52832_pac as pac;
use p_hal::{clocks::ClocksExt, gpio::{GpioExt,Level}};
use p_hal::{rng::RngExt, spim, twim, delay::Delay, clocks::LfOscConfiguration};



use arrayvec::ArrayString;
use core::fmt;
use core::fmt::Arguments;
use cortex_m_rt as rt;
use cortex_m_semihosting::hprintln;
use embedded_graphics::{prelude::*, primitives::*, style::*};
use embedded_graphics::{
    egtext, fonts::{Font12x16}, pixelcolor::Rgb565, text_style,
};
use embedded_hal::digital::v2::OutputPin;
use rt::entry;
use st7789::{Orientation, ST7789};
use cst816s::CST816S;

use embedded_hal::blocking::delay::{DelayMs,DelayUs};
use core::convert::TryInto;


pub type HalSpimError = p_hal::spim::Error;

pub type Spim0PortType = p_hal::spim::Spim<pac::SPIM0>;
pub type DisplaySckPinType =
p_hal::gpio::p0::P0_18<p_hal::gpio::Output<p_hal::gpio::PushPull>>;
pub type DisplayMosiPinType =
p_hal::gpio::p0::P0_26<p_hal::gpio::Output<p_hal::gpio::PushPull>>;


const SCREEN_WIDTH: i32 = 240;
const SCREEN_HEIGHT: i32 = 240;
const HALF_SCREEN_WIDTH: i32 = SCREEN_WIDTH / 2;
const MIN_SCREEN_DIM : i32 = SCREEN_HEIGHT;
const SCREEN_RADIUS: u32 = (MIN_SCREEN_DIM / 2) as u32;
const FONT_HEIGHT: i32 = 20; //for Font12x16


type DisplayType<'a> = st7789::ST7789<
    shared_bus::proxy::BusProxy<
        'a,
        cortex_m::interrupt::Mutex<core::cell::RefCell<Spim0PortType>>,
        Spim0PortType,
    >,
    DisplaySckPinType,
    DisplayMosiPinType,
>;

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
    // let _clockos = dp.CLOCK.constrain()
    //     .enable_ext_hfosc()
    //     //.set_lfclk_src_external(LfOscConfiguration::ExternalNoBypass)
    //     // TODO starting with external LFCLK hangs...
    //     .start_lfclk();

    let port0 = dp.P0.split();

    hprintln!("\r\n--- BEGIN ---").unwrap();

    // random number generator peripheral
    let mut rng = dp.RNG.constrain();

    // // pushbutton input GPIO: P0.13
    // let mut _user_butt = port0.p0_13.into_floating_input().degrade();
    // // must drive this pin high to enable pushbutton
    // let mut _user_butt_en =
    //     port0.p0_15.into_push_pull_output(Level::High).degrade();
    // vibration motor output: drive low to activate motor
    // let mut vibe = port0.p0_16.into_push_pull_output(Level::Low).degrade();
    // delay_source.delay_ms(100u8);
    // let _ = vibe.set_high();

    // internal i2c0 bus devices: BMA421 (accel), HRS3300 (hrs), CST816S (TouchPad)
    // BMA421-INT:  P0.08
    // TP-INT: P0.28
    let i2c0_pins = twim::Pins {
        scl: port0.p0_07.into_floating_input().degrade(),
        sda: port0.p0_06.into_floating_input().degrade(),
    };
    let i2c_port = twim::Twim::new(dp.TWIM1, i2c0_pins, twim::Frequency::K400);
    let i2c_bus0 = shared_bus::CortexMBusManager::new(i2c_port);

    delay_source.delay_ms(1u8);


    let spim0_pins = spim::Pins {
        sck: port0.p0_02.into_push_pull_output(Level::Low).degrade(),
        miso: None,
        mosi: Some(port0.p0_03.into_push_pull_output(Level::Low).degrade()),
    };

    // create SPIM0 interface, 8 Mbps, use 122 as "over read character"
    let spim0 = spim::Spim::new(
        dp.SPIM0,
        spim0_pins,
        spim::Frequency::M8,
        spim::MODE_3,
        122,
    );
    let spi_bus0 = shared_bus::CortexMBusManager::new(spim0);

    // backlight control pin for display: always on
    let mut _backlight = port0.p0_22.into_push_pull_output(Level::Low);
    // SPI chip select (CSN) for the display.
    let mut display_csn = port0.p0_25.into_push_pull_output(Level::High);

    // data/clock switch pin for display
    let display_dc = port0.p0_18.into_push_pull_output(Level::Low);
    // reset pin for display
    let display_rst = port0.p0_26.into_push_pull_output(Level::Low);

    // create display driver
    let mut display = ST7789::new(
        spi_bus0.acquire(),
        display_dc,
        display_rst,
        SCREEN_WIDTH as u16,
        SCREEN_HEIGHT as u16,
    );

    configure_display(&mut display, &mut display_csn, &mut delay_source);
    draw_background(&mut display, &mut display_csn);
    let half_height = SCREEN_HEIGHT / 2;
    let graph_area = Rectangle::new(
        Point::new(10, half_height - 50),
        Point::new(SCREEN_WIDTH - 20, half_height + 50),
    );

    let mut touchpad = CST816S::new(i2c_bus0.acquire());


    loop {

        let rando = [
            rng.random_u16() as i16,
            rng.random_u16() as i16,
            rng.random_u16() as i16,
        ];
        render_vec3_i16(
            &mut display,
            &mut display_csn,
             10,
            20,
            rando.as_ref(),
        );

        delay_source.delay_us(100u32);
    }
}


fn configure_display(display: &mut DisplayType,  display_csn: &mut impl OutputPin, delay_source: &mut (impl DelayMs<u8> + DelayUs<u32>)) {
    let _ = display_csn.set_low();
    display.init(delay_source).unwrap();
    display.set_orientation(&Orientation::Portrait).unwrap();
    let _ = display_csn.set_high();
}

fn draw_background(display: &mut DisplayType,  display_csn: &mut impl OutputPin) {
    if let Ok(_) = display_csn.set_low() {
        let clear_bg = Rectangle::new(
            Point::new(0, 0),
            Point::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK));
        clear_bg.draw(display).unwrap();

        let center_circle = Circle::new(
            Point::new(HALF_SCREEN_WIDTH, SCREEN_HEIGHT / 2),
            SCREEN_RADIUS,
        )
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 4));
        center_circle.draw(display).unwrap();
    }
    let _ = display_csn.set_high();
}

/// Render formatted text to the display
fn render_text(
    display: &mut DisplayType,
    display_csn: &mut impl OutputPin,
    x_pos: i32,
    y_pos: i32,
    color: Rgb565,
    args: Arguments<'_>,
) {
    let mut format_buf = ArrayString::<[u8; 16]>::new();
    if fmt::write(&mut format_buf, args).is_ok() {
        if display_csn.set_low().is_ok() {
            let _ = egtext!(
                text = &format_buf,
                top_left = Point::new(x_pos, y_pos),
                style = text_style!(
                    font = Font12x16,
                    text_color = color,
                    background_color = Rgb565::BLACK,
                )
            ).draw(display);
        }
        let _ = display_csn.set_high();
    }
}

/// render a vector of three i16 to the display
fn render_vec3_i16(
    display: &mut DisplayType,
    display_csn: &mut impl OutputPin,
    x_pos: i32,
    start_y: i32,
    buf: &[i16],
) {
    //TODO dynamically reformat depending on font size
    let mut y_pos = start_y;
    render_text(display, display_csn, x_pos,y_pos, Rgb565::GREEN,format_args!("X: {}", buf[0]));
    y_pos += FONT_HEIGHT;
    render_text(display, display_csn, x_pos,y_pos, Rgb565::GREEN, format_args!("Y: {}", buf[1]));
    y_pos += FONT_HEIGHT;
    render_text(display, display_csn, x_pos,y_pos, Rgb565::GREEN, format_args!("Z: {}", buf[2]));
}

fn render_graph_bar(
    display: &mut DisplayType,
    display_csn: &mut impl OutputPin,
    area: &Rectangle,
    x_pos: i32,
    value: f32, // this value
    avg: f32, // avg value
    color: Rgb565,
) {
    if display_csn.set_low().is_ok() {
        // clear rect
        let _ = Rectangle::new(Point::new(x_pos, area.top_left.y), Point::new(x_pos + 2, area.bottom_right.y))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(display);
        // actual bar
        //let y_off = (((area.bottom_right.y - area.top_left.y) as f32) * value) as i32;
        let half_height = (area.bottom_right.y - area.top_left.y)/2;
        let y_ctr =  area.top_left.y + half_height;
        let delta = (value - avg) / avg; //normalized delta
        let y_delta = (delta * (half_height  as f32)) as i32;
        let y_pos = y_ctr + y_delta;
        let _ = Rectangle::new(Point::new(x_pos, y_pos), Point::new(x_pos + 2, area.bottom_right.y))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(display);
    }
    let _ = display_csn.set_high();

    // display.draw( Rect::new(Coord::new(xpos, 0), Coord::new(xpos + (2*BAR_WIDTH), SCREEN_HEIGHT)).with_fill(Some(0u8.into())).into_iter());

}
