//! An unknown `#[zk_provable]` option must name the supported ones — a typo in
//! the option name would otherwise be silently ignored.

use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2", registar)]
fn typo_option(#[public] x: u64) {
    assert!(x >= 0);
}

fn main() {}