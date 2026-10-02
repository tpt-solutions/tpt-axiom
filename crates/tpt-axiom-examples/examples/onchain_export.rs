//! The on-chain verifier generator: prove `prove_balance_transfer` with the
//! real arkworks Groth16 backend, then emit a Solidity verifier (EIP-2537
//! BLS12-381 precompiles, verifying key hardcoded) plus a forge test with
//! the actual proof embedded.
//!
//! This is the "Solidity verifier generator" from the todo: the Rust side
//! owns the circuit and the proof; the emitted contract verifies proofs on
//! an EIP-2537-capable chain (Ethereum mainnet since Pectra, May 2025)
//! without any setup ceremony beyond Groth16's own per-circuit one.
//!
//! Wire format note: EIP-2537 encodes a field element as 64 big-endian
//! bytes, a G1 point as 128 bytes (`x || y`), and a G2 point as 256 bytes
//! (`x.c0 || x.c1 || y.c0 || y.c1`) — coordinates do **not** fit `uint256`,
//! so the contract's interface is `bytes`-based rather than the familiar
//! `alt_bn128` `uint256` pairs.
//!
//! Run from the repository root (regenerates the checked-in contracts):
//!
//! ```sh
//! cargo run -p tpt-axiom-examples --example onchain_export
//! cd contracts && forge test   # honest verify + tamper rejection on anvil
//! ```
//!
//! The generator function is long by necessity: it embeds the whole
//! verifier template.
//!
//! The emitted files are checked in (`contracts/Groth16Verifier.sol`,
//! `contracts/test/Groth16Verifier.t.sol`) so the forge suite runs without
//! Rust; regenerate them whenever the circuit changes.

use std::fmt::Write as _;

use ark_bls12_381::{Bls12_381, Fq, Fr};
use ark_ec::bls12::{G1Affine, G2Affine};
use ark_ff::PrimeField;
use ark_groth16::VerifyingKey;
use tpt_axiom_backend_arkworks::{ArkworksBackend, ArkworksProof};
use tpt_axiom_macros::zk_provable;
use tpt_axiom_zk::{CircuitDefinition, ZkBackend};

type G1 = G1Affine<ark_bls12_381::Config>;
type G2 = G2Affine<ark_bls12_381::Config>;

/// `ark_ff::Fp` → `0x` + 64-byte big-endian hex (the EIP-2537 Fp encoding:
/// 381-bit values left-padded with zeros).
fn fp_hex(fp: Fq) -> String {
    let limbs = fp.into_bigint().0; // little-endian u64 limbs
    let mut bytes = [0_u8; 64];
    for (i, limb) in limbs.iter().enumerate() {
        // limb i occupies the (i+1)-th 8-byte block from the right.
        let offset = 64 - 8 * (i + 1);
        bytes[offset..offset + 8].copy_from_slice(&limb.to_be_bytes());
    }
    let mut out = String::with_capacity(130);
    out.push_str("0x");
    for b in bytes {
        write!(out, "{b:02x}").unwrap();
    }
    out
}

/// G1 point → `(x, y)` hex pair, 64 bytes each.
fn g1_hex(p: G1) -> [String; 2] {
    [fp_hex(p.x), fp_hex(p.y)]
}

fn fp_bytes(fp: Fq) -> [u8; 64] {
    let limbs = fp.into_bigint().0;
    let mut bytes = [0_u8; 64];
    for (i, limb) in limbs.iter().enumerate() {
        let offset = 64 - 8 * (i + 1);
        bytes[offset..offset + 8].copy_from_slice(&limb.to_be_bytes());
    }
    bytes
}

fn g1_hex_bytes_bytes(p: G1) -> [u8; 128] {
    let (x, y) = (fp_bytes(p.x), fp_bytes(p.y));
    let mut out = [0_u8; 128];
    out[..64].copy_from_slice(&x);
    out[64..].copy_from_slice(&y);
    out
}

fn g2_hex_bytes_bytes(p: G2) -> [u8; 256] {
    let mut out = [0_u8; 256];
    out[..64].copy_from_slice(&fp_bytes(p.x.c0));
    out[64..128].copy_from_slice(&fp_bytes(p.x.c1));
    out[128..192].copy_from_slice(&fp_bytes(p.y.c0));
    out[192..].copy_from_slice(&fp_bytes(p.y.c1));
    out
}

fn g1_hex_bytes(p: G1) -> String {
    let [x, y] = g1_hex(p);
    format!("hex\"{}{}\"", &x[2..], &y[2..])
}

/// G2 point → `(x.c0, x.c1, y.c0, y.c1)` hex, 64 bytes each — the EIP-2537
/// element ordering.
fn g2_hex_bytes(p: G2) -> String {
    let parts = [
        fp_hex(p.x.c0),
        fp_hex(p.x.c1),
        fp_hex(p.y.c0),
        fp_hex(p.y.c1),
    ];
    let mut out = String::from("hex\"");
    for part in parts {
        out.push_str(&part[2..]);
    }
    out.push('"');
    out
}

#[zk_provable(backend = "arkworks")]
/// The on-chain circuit: the same balance-transfer statement the other
/// examples prove.
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

fn main() {
    let backend = ArkworksBackend;
    let ir = ProveBalanceTransfer.build();
    let circuit = backend.compile(&ir).expect("compile");
    let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");

    // A real proof over the honest witness, self-checked.
    let proof = backend
        .prove(&circuit, &pk, &[50, 20], &[30])
        .expect("honest witness must prove");
    assert!(backend.verify(&vk, &[50, 20], &proof).expect("verify"));

    std::fs::create_dir_all("contracts/test").expect("create contracts dir");
    std::fs::write("contracts/Groth16Verifier.sol", verifier_source(&vk)).expect("write verifier");

    // The exact pairing input the Solidity contract will hand to precompile
    // 0x0f. Emitted here (same run, same keys, same proof) and embedded in
    // the generated test, so the Solidity construction is compared with the
    // Rust construction byte for byte — no cross-run ambiguity.
    let neg_a = -proof.0.a;
    let vkx = (vk.gamma_abc_g1[0]
        + (vk.gamma_abc_g1[1] * Fr::from(50_u64))
        + (vk.gamma_abc_g1[2] * Fr::from(20_u64)))
    .into();
    let mut input = Vec::new();
    input.extend_from_slice(&g1_hex_bytes_bytes(neg_a));
    input.extend_from_slice(&g2_hex_bytes_bytes(proof.0.b));
    input.extend_from_slice(&g1_hex_bytes_bytes(vk.alpha_g1));
    input.extend_from_slice(&g2_hex_bytes_bytes(vk.beta_g2));
    input.extend_from_slice(&g1_hex_bytes_bytes(vkx));
    input.extend_from_slice(&g2_hex_bytes_bytes(vk.gamma_g2));
    input.extend_from_slice(&g1_hex_bytes_bytes(proof.0.c));
    input.extend_from_slice(&g2_hex_bytes_bytes(vk.delta_g2));
    assert_eq!(input.len(), 1536);
    let mut hex = String::from("0x");
    for b in &input {
        write!(hex, "{b:02x}").unwrap();
    }
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pairing_input.txt");
    std::fs::write(&fixture, hex).expect("write fixture");

    std::fs::write(
        "contracts/test/Groth16Verifier.t.sol",
        test_source(&proof, &input),
    )
    .expect("write test");

    println!("wrote contracts/Groth16Verifier.sol + contracts/test/Groth16Verifier.t.sol");
    println!("next: cd contracts && forge test");
}

/// Emits the fixed-circuit verifier: verifying key hardcoded (the
/// generated-verifier convention), pairing check on the EIP-2537
/// precompiles.
#[allow(clippy::too_many_lines)] // a full Solidity template, inherently long
fn verifier_source(vk: &VerifyingKey<Bls12_381>) -> String {
    let mut ic_consts = String::new();
    let mut vkx_code = String::new();
    for (i, point) in vk.gamma_abc_g1.iter().enumerate() {
        writeln!(
            ic_consts,
            "    bytes constant IC_{i} = {};",
            g1_hex_bytes(*point)
        )
        .unwrap();
        if i > 0 {
            // IC_0 carries the constant one; IC_i binds publics[i-1].
            writeln!(
                vkx_code,
                "        vkx = g1_add(vkx, g1_mul(IC_{i}, publics[{i} - 1]));"
            )
            .unwrap();
        }
    }
    let alpha = g1_hex_bytes(vk.alpha_g1);
    let beta = g2_hex_bytes(vk.beta_g2);
    let gamma = g2_hex_bytes(vk.gamma_g2);
    let delta = g2_hex_bytes(vk.delta_g2);
    let public_count = vk.gamma_abc_g1.len() - 1;

    format!(
        r#"// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.20;

/// GENERATED by tpt-axiom's onchain_export example from the arkworks
/// verifying key of `prove_balance_transfer`. Do not edit by hand; run the
/// exporter again when the circuit changes.
///
/// Groth16 verification over BLS12-381 through the EIP-2537 precompiles
/// (live on Ethereum mainnet since Pectra, May 2025). The pairing
/// precompile performs the subgroup checks.
///
/// Wire format (EIP-2537): field elements are 64 big-endian bytes, G1
/// points are 128 bytes (`x || y`), G2 points are 256 bytes
/// (`x.c0 || x.c1 || y.c0 || y.c1`). Coordinates exceed `uint256`, so the
/// interface is bytes-based.
///
/// Security note: Groth16 has a per-circuit trusted setup. This contract is
/// only as trustworthy as the ceremony behind the hardcoded verifying key —
/// see the repository's SECURITY.md.
contract Groth16Verifier {{
    // BLS12-381 base field modulus (381 bits) as three 128-bit chunks.
    uint256 constant CHUNK0 = 0x1eabfffeb153ffffb9feffffffffaaab;
    uint256 constant CHUNK1 = 0x64774b84f38512bf6730d2a0f6b0f624;
    uint256 constant CHUNK2 = 0x1a0111ea397fe69a4b1ba7b6434bacd7;
    uint256 constant CHUNK_MASK = 0xffffffffffffffffffffffffffffffff;
    address constant G1_ADD = address(0x0b);
    address constant G1_MUL = address(0x0c);
    address constant PAIRING = address(0x0f); // EIP-2537 pairing check

    // ---- verifying key (hardcoded) --------------------------------------
{ic_consts}
    bytes constant ALPHA = {alpha};
    bytes constant BETA = {beta};
    bytes constant GAMMA = {gamma};
    bytes constant DELTA = {delta};

    function g1_add(bytes memory p, bytes memory q)
        internal view returns (bytes memory)
    {{
        (bool ok, bytes memory out) = G1_ADD.staticcall(abi.encodePacked(p, q));
        require(ok && out.length == 128, "g1 add failed");
        return out;
    }}

    function g1_mul(bytes memory p, uint256 s)
        internal view returns (bytes memory)
    {{
        (bool ok, bytes memory out) =
            G1_MUL.staticcall(abi.encodePacked(p, bytes32(s))); // MSM: point, then scalar
        require(ok && out.length == 128, "g1 mul failed");
        return out;
    }}

    /// Negates a G1 point: (x, y) -> (x, p - y), with p and y split into
    /// three 128-bit chunks (p is 381 bits = 3 x 128). The identity (all
    /// zeros) negates to itself.
    function g1_negate(bytes memory p) internal pure returns (bytes memory) {{
        require(p.length == 128, "g1 point must be 128 bytes");
        bytes32 xHi;
        bytes32 xLo;
        bytes32 yHi;
        bytes32 yLo;
        assembly {{
            xHi := mload(add(p, 32))
            xLo := mload(add(p, 64))
            yHi := mload(add(p, 96))
            yLo := mload(add(p, 128))
        }}
        if (yHi == bytes32(0) && yLo == bytes32(0)) {{
            return p; // the identity
        }}
        unchecked {{
            // y = yc2·2^256 + yc1·2^128 + yc0; p = pc2·2^256 + pc1·2^128 + pc0.
            uint256 yc0 = uint256(yLo) & CHUNK_MASK;
            uint256 yc1 = uint256(yLo) >> 128;
            uint256 yc2 = uint256(yHi);
            bool borrow = yc0 > CHUNK0;
            uint256 d0 = CHUNK0 - yc0;
            bool b1 = yc1 > CHUNK1 || (yc1 == CHUNK1 && borrow);
            uint256 d1 = CHUNK1 - yc1 - (borrow ? 1 : 0);
            bool b2 = yc2 > CHUNK2 || (yc2 == CHUNK2 && b1);
            uint256 d2 = CHUNK2 - yc2 - (b1 ? 1 : 0);
            require(!b2, "negation underflow (y >= p)");
            // Reassemble the 64-byte coordinate: the y field is
            // [16 zero bytes | d2 | d1 | d0] (p < 2^381, d2 < 2^125).
            return abi.encodePacked(
                xHi,
                xLo,
                bytes16(0),
                bytes16(uint128(d2)),
                bytes16(uint128(d1)),
                bytes16(uint128(d0))
            );
        }}
    }}

    /// DEBUG: the exact bytes handed to the pairing precompile (used by the
    /// generated test to diff the Solidity construction against the Rust
    /// construction byte for byte).
    function debugPairingInput(
        bytes calldata a,
        bytes calldata b,
        bytes calldata c,
        uint256[{public_count}] calldata publics
    ) public view returns (bytes memory) {{
        require(a.length == 128 && b.length == 256 && c.length == 128, "bad encoding");
        bytes memory vkx = IC_0;
{vkx_code}
        bytes memory negA = g1_negate(a);
        return abi.encodePacked(
            negA, b,
            ALPHA, BETA,
            vkx, GAMMA,
            c, DELTA
        );
    }}

    /// Groth16: e(-A, B) · e(alpha, beta) · e(vk_x, gamma) · e(C, delta) == 1,
    /// where vk_x = IC_0 + Σ pub_i · IC_i.
    function verifyProof(
        bytes calldata a, // 128-byte G1 point
        bytes calldata b, // 256-byte G2 point
        bytes calldata c, // 128-byte G1 point
        uint256[{public_count}] calldata publics
    ) public view returns (bool) {{
        require(a.length == 128 && b.length == 256 && c.length == 128, "bad encoding");
        bytes memory vkx = IC_0;
{vkx_code}
        bytes memory negA = g1_negate(a);

        bytes memory input = abi.encodePacked(
            negA, b,
            ALPHA, BETA,
            vkx, GAMMA,
            c, DELTA
        );
        (bool callOk, bytes memory out) = PAIRING.staticcall(input);
        return callOk && out.length == 32 && abi.decode(out, (uint256)) == 1;
    }}
}}
"#
    )
}

/// Emits the forge test: the real proof, the honest verdict, and a
/// tampered-publics case that must fail.
fn test_source(proof: &ArkworksProof, pairing_input: &[u8]) -> String {
    let [ax, ay] = g1_hex(proof.0.a);
    let a_bytes = format!("hex\"{}{}\"", &ax[2..], &ay[2..]);
    let b_bytes = g2_hex_bytes(proof.0.b);
    let [cx, cy] = g1_hex(proof.0.c);
    let c_bytes = format!("hex\"{}{}\"", &cx[2..], &cy[2..]);
    let input_bytes = {
        let mut out = String::from("hex\"");
        for b in pairing_input {
            write!(out, "{b:02x}").unwrap();
        }
        out.push('"');
        out
    };

    format!(
        r#"// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity >=0.8.20;

import "../Groth16Verifier.sol";
import "forge-std/Test.sol";

/// GENERATED by tpt-axiom's onchain_export example: a real arkworks Groth16
/// proof over `prove_balance_transfer` with publics [50, 20].
contract Groth16VerifierProofTest is Test {{
    Groth16Verifier internal verifier;

    constructor() {{
        verifier = new Groth16Verifier();
    }}

    bytes constant A = {a_bytes};
    bytes constant B = {b_bytes};
    bytes constant C = {c_bytes};
    bytes constant EXPECTED_PAIRING_INPUT = {input_bytes};

    function test_input_matches_rust_byte_for_byte() external view {{
        bytes memory got = verifier.debugPairingInput(A, B, C, [uint256(50), uint256(20)]);
        if (keccak256(got) != keccak256(EXPECTED_PAIRING_INPUT)) {{
            for (uint256 i = 0; i < got.length && i < EXPECTED_PAIRING_INPUT.length; i++) {{
                if (got[i] != EXPECTED_PAIRING_INPUT[i]) {{
                    revert(
                        string(
                            abi.encodePacked(
                                "DIFF at byte ",
                                vm.toString(i),
                                " (pair ",
                                vm.toString(i / 384),
                                "); got ",
                                vm.toString(got.length),
                                " bytes, expected ",
                                vm.toString(EXPECTED_PAIRING_INPUT.length)
                            )
                        )
                    );
                }}
            }}
            revert("same length, different content");
        }}
    }}

    function test_honest_proof_verifies() external view {{
        uint256[2] memory publics = [uint256(50), uint256(20)];
        require(verifier.verifyProof(A, B, C, publics), "the real proof must verify on-chain");
    }}

    function test_tampered_publics_rejected() external view {{
        uint256[2] memory bad = [uint256(21), uint256(20)]; // one public flipped
        require(
            !verifier.verifyProof(A, B, C, bad),
            "tampered publics must be rejected"
        );
    }}
}}
"#
    )
}
