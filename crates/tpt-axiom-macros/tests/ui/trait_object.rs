use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn takes_dyn(#[secret] value: &dyn core::fmt::Debug) {
    let _ = value;
}

fn main() {}
