use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn signed_division(x: i64, #[secret] d: i64) -> i64 {
    x / d
}

fn main() {}
