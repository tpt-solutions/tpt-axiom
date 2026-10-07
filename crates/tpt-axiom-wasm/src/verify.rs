//! In-browser Groth16 verification.
//!
//! The proving stacks stay native-only, but *verification* is just a pairing
//! check over shipped key material, so it runs fine in WASM. This module lets
//! the playground do exactly what a verifier process does: take a verifying
//! key, a proof and the public inputs as hex, and answer `valid`/`invalid`
//! with no trust in the prover whatsoever.
//!
//! The shipped fixture is a real proof — a Groth16/BLS12-381 setup over the
//! `weighted_sum` circuit (`result == a * k`, `k` secret) with public inputs
//! `a = 2`, `result = 8`, produced by native proving (see the
//! `generate_fixture` test in `lib.rs`, which regenerates it). Editing any of
//! the three inputs in the page flips the verdict to `invalid`.

use ark_bls12_381::Bls12_381;
use ark_groth16::{Groth16, Proof, VerifyingKey};
use ark_serialize::CanonicalDeserialize;
use ark_snark::SNARK;
use tpt_axiom_backend_arkworks::encode_scalar;
use wasm_bindgen::prelude::*;

/// Honest public inputs of the demo proof: `a = 2`, `result = 8`.
pub const DEMO_PUBLIC_INPUTS: [i64; 2] = [2, 8];

/// Compressed verifying key of the demo circuit (`weighted_sum`), hex.
pub const DEMO_VK_HEX: &str = "a9818688970fcc28343e6af2822680d463af26a3c83d28bceae57019aca20e3ce3c94bbc4f01321b11c8df2d9c6517a0b8c36d004337485a386c6651af186c4ea7714f67329b68304cd79947ae884e1dccde0cae89dfac5f70716efd6d0c4a4d0fa64beb1624fb656ac9b425460add33e9f82a55b06ddb0e9f4a362b9b0cab9ef0740aa674294733bc742b709cd04a5c864055cf037c802a1443882d9b1d49dd31c88b84a7658b997eb6c1f4967e31ed01a0ed505c036685d45f3a3d6b3469620ddeac45ffa8e7ca0c0f72c1b4c4ced7493a4fdfdb3da29929482d76887ec5f62c84969abb1a26ed17ede884e07d84da8bd96c9b146ceb0f5c781591f3c75a2443c493fce64332b162bb31e669f3701242bfe4ef785bcd6e6cc72a2ccd3550150a04406b01d595b367f26c2f3435c7e12e18d99edb125c9a13e24512df10ed2dfbb04219ce03e768f31fd0a1c2b1077a030000000000000080f324bfd6c4116201d2cb43cd1c3a9ed2b03d440deadda9a9642e5f5fc832e04ca0ded55964fc94379c9d35ef301810b9301257b38c43801f4c33c193b052880e49e5100067788a25a195892b5a26d0e8c07698b75421c485251aae653ef4a18238ade4b73dfc876cbf7c01f80ced1f724dd827f7f63f213b375743fb7e49753542ef883b5f01f4f26c8dd53ca050da";

/// Compressed Groth16 proof for the demo circuit over the honest public
/// inputs, hex.
pub const DEMO_PROOF_HEX: &str = "98839925b32ea283b392968c3bfbe1f4b0b267ff459d0e70ec89da5831305a4cfca79484718bfa7f48591cae98b5618c98c28453e8c34cd3d4f4ef55554e5f16c4640e722e5851e5d73958c9440c45c961796fd11b6a596a507b82d1b5d74993077c8a25795b2ceac08e23ae45bb7e9ffff4e041df13be0ce242160ade66de97abd292d4587efa8d01da23e884af455098ce46d4cc4ade1c1f14d870693541e925822d4947db1ff62bb5ea41b4c2cde4c9556b3bb09e05b2b89ae141b105f714";

/// A human-readable description of the shipped demo proof, for the page.
#[wasm_bindgen]
pub fn demo_proof_info() -> String {
    format!(
        "circuit: weighted_sum  (result == a * k, k secret)\npublic inputs: a = {}, result = {}\nverifying key: {} bytes (compressed, BLS12-381 Groth16)\nproof: {} bytes",
        DEMO_PUBLIC_INPUTS[0],
        DEMO_PUBLIC_INPUTS[1],
        DEMO_VK_HEX.len() / 2,
        DEMO_PROOF_HEX.len() / 2,
    )
}

/// The shipped demo's verifying key, hex — so the page can load it.
#[wasm_bindgen]
pub fn demo_verifying_key_hex() -> String {
    DEMO_VK_HEX.to_string()
}

/// The shipped demo's proof, hex.
#[wasm_bindgen]
pub fn demo_proof_hex() -> String {
    DEMO_PROOF_HEX.to_string()
}

/// The shipped demo's honest public inputs, comma-separated.
#[wasm_bindgen]
pub fn demo_public_inputs() -> String {
    DEMO_PUBLIC_INPUTS
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
/// Verifies a Groth16 proof over BLS12-381 in the browser.
///
/// All three arguments are strings: the compressed verifying key as hex, the
/// compressed proof as hex, and the comma-separated public inputs as decimal
/// `i64` (the same encoding the native backends use). Returns `"valid"` or
/// `"invalid"`; a malformed or truncated artifact is a thrown error rather
/// than a silent `false`, because "this was not a proof at all" and "this
/// proof does not hold" are different answers.
///
/// # Errors
/// Throws when the hex is malformed, the key or proof does not deserialize,
/// the public-input count disagrees with the key, or the pairing check cannot
/// be evaluated.
#[wasm_bindgen]
pub fn groth16_verify(
    vk_hex: &str,
    proof_hex: &str,
    public_inputs: &str,
) -> Result<String, JsValue> {
    verify(vk_hex, proof_hex, public_inputs).map_err(|e| JsValue::from_str(&e))
}

/// The platform-independent core of [`groth16_verify`]: same logic, ordinary
/// `String` errors, so it is testable natively as well as in the browser.
fn verify(vk_hex: &str, proof_hex: &str, public_inputs: &str) -> Result<String, String> {
    let vk: VerifyingKey<Bls12_381> = deserialize(vk_hex, "verifying key")?;
    let proof: Proof<Bls12_381> = deserialize(proof_hex, "proof")?;

    let mut parsed = Vec::new();
    for token in public_inputs.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let value: i64 = token
            .parse()
            .map_err(|_| format!("public input {token:?} is not an i64 decimal integer"))?;
        parsed.push(encode_scalar(value));
    }

    // The key carries one curve point per public input plus the trapping
    // element; a mismatch is a false claim about the circuit, not a tool error.
    let expected = vk.gamma_abc_g1.len().saturating_sub(1);
    if parsed.len() != expected {
        return Err(format!(
            "verifying key expects {expected} public inputs, got {}",
            parsed.len()
        ));
    }

    let ok = Groth16::<Bls12_381>::verify(&vk, &parsed, &proof)
        .map_err(|e| format!("pairing check failed: {e}"))?;
    Ok(if ok { "valid" } else { "invalid" }.to_string())
}

fn deserialize<T: CanonicalDeserialize>(hex_str: &str, what: &str) -> Result<T, String> {
    let bytes = from_hex(hex_str)?;
    // Trailing bytes mean the artifact was truncated or two artifacts were
    // concatenated; accepting it would verify against a prefix of the intent.
    let mut cursor = bytes.as_slice();
    let value = T::deserialize_compressed(&mut cursor)
        .map_err(|e| format!("{what} is not a valid compressed artifact: {e}"))?;
    if !cursor.is_empty() {
        return Err(format!(
            "{what} has {} trailing byte(s): the artifact is truncated or concatenated",
            cursor.len()
        ));
    }
    Ok(value)
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let cleaned = cleaned.strip_prefix("0x").unwrap_or(&cleaned);
    if cleaned.len() % 2 != 0 {
        return Err("hex string has an odd number of digits".to_string());
    }
    let bytes = cleaned.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = hex_digit(pair[0])?;
        let lo = hex_digit(pair[1])?;
        out.push(hi << 4 | lo);
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        other => Err(format!("{:?} is not a hex digit", other as char)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_demo_proof_verifies() {
        let verdict = verify(DEMO_VK_HEX, DEMO_PROOF_HEX, "2, 8").expect("verify");
        assert_eq!(verdict, "valid");
    }

    #[test]
    fn tampered_public_input_is_invalid() {
        // result = 9 is the near-miss forgery: the secret k = 4 no longer
        // explains it.
        let verdict = verify(DEMO_VK_HEX, DEMO_PROOF_HEX, "2, 9").expect("verify");
        assert_eq!(verdict, "invalid");
    }

    #[test]
    fn tampered_proof_is_rejected() {
        // Flip one nibble of the proof's leading byte.
        let mut bytes: Vec<char> = DEMO_PROOF_HEX.chars().collect();
        bytes[0] = if bytes[0] == '9' { '8' } else { '9' };
        let tampered: String = bytes.into_iter().collect();
        let outcome = verify(DEMO_VK_HEX, &tampered, "2, 8");
        // Either the bytes stop deserializing or the pairing check rejects it;
        // both are refusals, neither is a pass.
        assert!(
            outcome.as_deref() != Ok("valid"),
            "tampered proof must not verify"
        );
    }

    #[test]
    fn truncated_proof_is_an_error_not_a_false() {
        let truncated = &DEMO_PROOF_HEX[..DEMO_PROOF_HEX.len() - 2];
        assert!(verify(DEMO_VK_HEX, truncated, "2, 8").is_err());
    }

    #[test]
    fn wrong_public_input_count_is_an_error() {
        assert!(verify(DEMO_VK_HEX, DEMO_PROOF_HEX, "2").is_err());
        assert!(verify(DEMO_VK_HEX, DEMO_PROOF_HEX, "2, 8, 1").is_err());
    }

    #[test]
    fn malformed_hex_is_an_error() {
        assert!(verify("zz", DEMO_PROOF_HEX, "2, 8").is_err());
        assert!(verify(DEMO_VK_HEX, DEMO_PROOF_HEX, "two, eight").is_err());
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Regenerates a playground-style fixture from scratch — fresh Groth16
    /// setup, real proof — and checks it through the *same* function the page
    /// calls. The committed constants above were produced this way, so the
    /// shipped demo is a genuine proof rather than hand-written bytes. Because
    /// the setup is randomized, a re-run yields a different but equally valid
    /// proof; only the shape of the verdict is asserted.
    #[test]
    fn freshly_proven_fixture_verifies_through_the_browser_path() {
        use tpt_axiom_zk::ZkBackend;

        let backend = tpt_axiom_backend_arkworks::ArkworksBackend;
        let ir = crate::demo_ir("weighted_sum").expect("demo circuit");
        let circuit = backend.compile(&ir).expect("compile");
        let (pk, vk) = backend.generate_keys(&ir, &[]).expect("keys");
        let proof = backend.prove(&circuit, &pk, &[2, 8], &[4]).expect("prove");

        let vk_hex = hex(&tpt_axiom_backend_arkworks::to_bytes(&vk).expect("vk bytes"));
        let proof_hex = hex(&backend.encode_proof(&proof).expect("proof bytes"));

        assert_eq!(
            verify(&vk_hex, &proof_hex, "2, 8").expect("verify"),
            "valid"
        );
        assert_eq!(
            verify(&vk_hex, &proof_hex, "2, 9").expect("verify"),
            "invalid",
            "the near-miss public input must be refused"
        );
    }
}
