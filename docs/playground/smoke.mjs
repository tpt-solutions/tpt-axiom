import init, { groth16_verify, demo_verifying_key_hex, demo_proof_hex, demo_public_inputs, demo_proof_info }
  from "./pkg/tpt_axiom_wasm.js";
import { readFile } from "node:fs/promises";
// `--target web` normally fetches the .wasm; under Node we hand the bytes over
// directly so the smoke test needs no HTTP server.
await init({ module_or_path: await readFile(new URL("./pkg/tpt_axiom_wasm_bg.wasm", import.meta.url)) });
const vk = demo_verifying_key_hex();
const proof = demo_proof_hex();
const ok = groth16_verify(vk, proof, demo_public_inputs());
const nearmiss = groth16_verify(vk, proof, "2, 9");
let tampered;
try { groth16_verify(vk, proof.slice(0, -2) + "00", demo_public_inputs()); tampered = "PASSED(bad)"; }
catch (e) { tampered = "threw: " + e; }
console.log(demo_proof_info());
console.log("honest:", ok, "| near-miss:", nearmiss, "| truncated:", tampered);
if (ok !== "valid" || nearmiss !== "invalid" || tampered.startsWith("PASSED")) { process.exit(1); }