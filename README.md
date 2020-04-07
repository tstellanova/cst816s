## cst816s 


A rust no_std driver for the
Hynan CST816S touchpad device

## Status
This is work-in-progress
- [ ] Debug build runs on PineTime
- [ ] Release build runs on PineTime
- [ ] Internal I2C bus access
- [ ] CI
- [ ] Documentation

## Examples

The following example was developed for and tested on the 
[PineTime](https://wiki.pine64.org/index.php/PineTime)
nrf52-based smart watch.

Note that you will need to 
[clear the nrf52 flash protection](https://gist.github.com/tstellanova/8c8509ae3dd4f58697c3b487dc3393b2)
before you will be able to program the PineTime. 

For installation and debugging you can connect with the PineTime on its SWD debug port using, for example:
- openocd (built with proper support). We've used an inexpensive ST-Link adapter to with openocd. 
- [daily build of the Black Magic Probe firmware](https://github.com/blacksphere/blackmagic/wiki/Upgrading-Firmware)
- Segger J-Link or similar

When you're ready to install on the PineTime, you can run the example with:

```
cargo run --example touchpad
```

## Resources
- [Datasheet](./CST816S_V1.1.en.pdf)

## License

BSD-3-Clause, see `LICENSE` file. 