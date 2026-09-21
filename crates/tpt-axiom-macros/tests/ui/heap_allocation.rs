use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn make_string(#[secret] n: u64) {
    let _s = String::new();
    assert!(n >= 0);
}

fn main() {}
