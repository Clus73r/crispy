extern crate crispy;

use crispy::run;

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    pollster::block_on(run());
}
