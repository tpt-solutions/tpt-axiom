//! Cross-checks the on-chain verifier's pairing equation against arkworks:
//! builds the exact EIP-2537 pairing input the generated Solidity contract
//! sends, decodes it back into ark points, and asserts the product of the
//! four pairings is one. Also exposes the raw input bytes as a test fixture
//! for the precompile-level `cast` debug path.

use ark_bls12_381::{Bls12_381, Fq, Fq2};
use ark_ec::bls12::{G1Affine, G2Affine};
use ark_ec::pairing::Pairing;
use ark_ff::PrimeField;
use tpt_axiom_backend_arkworks::{ArkworksBackend, ArkworksProof};
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

type G1 = G1Affine<ark_bls12_381::Config>;
type G2 = G2Affine<ark_bls12_381::Config>;

#[zk_provable(backend = "arkworks")]
#[allow(clippy::missing_const_for_fn)] // kept fn shape is fixed by the macro
fn prove_balance_transfer(
    #[public] sender_balance: u64,
    #[public] receiver_balance: u64,
    #[secret] amount: u64,
) {
    assert!(sender_balance >= amount);
    let new_receiver_balance = receiver_balance + amount;
    assert_eq!(new_receiver_balance, receiver_balance + amount);
}

/// EIP-2537 Fp → 64-byte big-endian.
fn fp_bytes(fp: Fq) -> [u8; 64] {
    let limbs = fp.into_bigint().0;
    let mut bytes = [0_u8; 64];
    for (i, limb) in limbs.iter().enumerate() {
        let offset = 64 - 8 * (i + 1);
        bytes[offset..offset + 8].copy_from_slice(&limb.to_be_bytes());
    }
    bytes
}

fn g1_bytes(p: G1) -> [u8; 128] {
    let (x, y) = (fp_bytes(p.x), fp_bytes(p.y));
    let mut out = [0_u8; 128];
    out[..64].copy_from_slice(&x);
    out[64..].copy_from_slice(&y);
    out
}

fn g2_bytes(p: G2) -> [u8; 256] {
    let mut out = [0_u8; 256];
    out[..64].copy_from_slice(&fp_bytes(p.x.c0));
    out[64..128].copy_from_slice(&fp_bytes(p.x.c1));
    out[128..192].copy_from_slice(&fp_bytes(p.y.c0));
    out[192..].copy_from_slice(&fp_bytes(p.y.c1));
    out
}

/// Decodes an EIP-2537 G1 point back into ark.
fn g1_from_bytes(bytes: &[u8]) -> G1 {
    assert_eq!(bytes.len(), 128);
    let x = Fq::from_be_bytes_mod_order(&bytes[..64]);
    let y = Fq::from_be_bytes_mod_order(&bytes[64..]);
    if bytes.iter().all(|&b| b == 0) {
        return G1::identity();
    }
    G1 {
        x,
        y,
        infinity: false,
    }
}

fn g2_from_bytes(bytes: &[u8]) -> G2 {
    assert_eq!(bytes.len(), 256);
    if bytes.iter().all(|&b| b == 0) {
        return G2::identity();
    }
    let c0 = |slice: &[u8]| Fq::from_be_bytes_mod_order(slice);
    G2 {
        x: Fq2::new(c0(&bytes[..64]), c0(&bytes[64..128])),
        y: Fq2::new(c0(&bytes[128..192]), c0(&bytes[192..])),
        infinity: false,
    }
}

#[test]
fn the_contract_pairing_input_satisfies_groth16_in_ark() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("prove");
    let ArkworksProof(inner) = &proof;
    assert!(backend.verify(&vk, &[50, 20], &proof).expect("verify"));

    // The exact bytes the Solidity contract sends to precompile 0x0f:
    // (negA, b), (alpha, beta), (vkx, gamma), (c, delta).
    let mut input = Vec::new();
    input.extend_from_slice(&g1_bytes(-inner.a)); // negA
    input.extend_from_slice(&g2_bytes(inner.b));
    input.extend_from_slice(&g1_bytes(vk.alpha_g1));
    input.extend_from_slice(&g2_bytes(vk.beta_g2));

    // vkx = IC_0 + 50*IC_1 + 20*IC_2, accumulated in affine arithmetic the
    // way the contract's precompile calls would (IC_0 first, like the
    // contract's `vkx = IC_0` start).
    let step = |acc: G1, ic: G1, s: u64| -> G1 {
        // G1's scalar field is BLS12-381's Fr.
        (acc + (ic * ark_bls12_381::Fr::from(s))).into()
    };
    let vkx: G1 = step(
        step(vk.gamma_abc_g1[0], vk.gamma_abc_g1[1], 50),
        vk.gamma_abc_g1[2],
        20,
    );
    input.extend_from_slice(&g1_bytes(vkx));
    input.extend_from_slice(&g2_bytes(vk.gamma_g2));

    input.extend_from_slice(&g1_bytes(inner.c));
    input.extend_from_slice(&g2_bytes(vk.delta_g2));

    assert_eq!(input.len(), 4 * 384, "4 pairs of (G1, G2)");

    // Decode the input back and run the pairing product exactly as the
    // precompile would.
    {
        use ark_ff::Zero;

        let p1 = g1_from_bytes(&input[0..128]);
        let q1 = g2_from_bytes(&input[128..384]);
        let p2 = g1_from_bytes(&input[384..512]);
        let q2 = g2_from_bytes(&input[512..768]);
        let p3 = g1_from_bytes(&input[768..896]);
        let q3 = g2_from_bytes(&input[896..1152]);
        let p4 = g1_from_bytes(&input[1152..1280]);
        let q4 = g2_from_bytes(&input[1280..1536]);
        // Decode roundtrips: each decoded point must equal the original.
        assert_eq!(p1, -inner.a, "negA roundtrip");
        assert_eq!(q1, inner.b, "b roundtrip");
        assert_eq!(p2, vk.alpha_g1, "alpha roundtrip");
        assert_eq!(q2, vk.beta_g2, "beta roundtrip");
        assert_eq!(p4, inner.c, "c roundtrip");
        assert_eq!(q4, vk.delta_g2, "delta roundtrip");
        // vkx must equal ark's own prepared-input accumulation.
        let ark_vkx: G1 = (vk.gamma_abc_g1[0]
            + (vk.gamma_abc_g1[1] * ark_bls12_381::Fr::from(50u64))
            + (vk.gamma_abc_g1[2] * ark_bls12_381::Fr::from(20u64)))
        .into();
        assert_eq!(p3, ark_vkx, "vkx mismatch");
        let product = Bls12_381::multi_pairing([p1, p2, p3, p4], [q1, q2, q3, q4]);
        let product_is_one = product.is_zero();
        // Negation sanity: the un-negated equation must NOT hold.
        let without_neg = Bls12_381::multi_pairing(
            [inner.a, vk.alpha_g1, vkx, inner.c],
            [inner.b, vk.beta_g2, vk.gamma_g2, vk.delta_g2],
        );
        assert!(!without_neg.is_zero(), "un-negated product must not be one");
        assert!(
            product_is_one,
            "the contract's pairing input must satisfy Groth16"
        );
    }

    // Emit the exact bytes for the precompile-level cast debug path.
    let mut hex = String::from("0x");
    for b in &input {
        use std::fmt::Write as _;
        write!(hex, "{b:02x}").unwrap();
    }
    let manifest = env!("CARGO_MANIFEST_DIR");
    let out = std::path::Path::new(manifest).join("../../target/pairing_input.txt");
    std::fs::write(out, hex).unwrap();
}
