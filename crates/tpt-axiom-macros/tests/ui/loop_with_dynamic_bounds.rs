use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn sum_up_to(#[secret] n: u64) {
    for _ in 0..n {
        assert!(n >= 0);
    }
}

fn main() {}
