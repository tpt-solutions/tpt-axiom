use tpt_axiom_macros::zk_provable;

#[zk_provable(backend = "halo2")]
fn withdraw(#[public] balance: u64, #[secret] amount: u64) {
    return;
    assert!(balance >= amount);
}

fn main() {}
