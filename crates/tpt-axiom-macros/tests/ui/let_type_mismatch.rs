//! A `let` type ascription that disagrees with the declared type of the
//! binding it initialises must be a compile error, not silently ignored.

use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn conflicting_ascription(#[public] amount: u8) {
    let wrong: u16 = amount;
    let _ = wrong;
}

fn main() {}