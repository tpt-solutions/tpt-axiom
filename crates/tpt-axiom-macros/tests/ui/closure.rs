use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn apply(#[secret] n: u64) {
    let f = |x: u64| x + 1;
    assert!(f(n) >= 0);
}

fn main() {}
