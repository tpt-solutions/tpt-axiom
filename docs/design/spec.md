# Design Document: `tpt-axiom`
## Probabilistic Logic and Native Zero-Knowledge State

### 1. Overview
`tpt-axiom` brings advanced mathematical reasoning natively into Rust. It bridges the gap between deterministic systems programming and the stochastic, provable mathematics required by modern autonomous systems, quantitative finance, and Web3. It allows developers to write probabilistic algorithms and Zero-Knowledge circuits using standard Rust syntax, with the compiler handling the heavy mathematical lifting.

### 2. Core Primitives
#### 2.1 Probabilistic & Uncertainty Types
Natively represents data that is not 100% certain, automatically propagating variance and confidence intervals.
* **Mechanism:** Types like `Fuzzy<T>` or `Distribution<T>` wrap a value and its variance. When math operations are performed, the compiler automatically applies the rules of probability (e.g., error propagation) to calculate the new variance.
* **Use Case:** Kalman filters, Monte Carlo simulations, and sensor fusion in robotics become as easy as writing standard arithmetic.

#### 2.2 Native Zero-Knowledge (ZK) State Primitives
Eliminates the disconnect between application logic and ZK arithmetic circuits.
* **Mechanism:** A `#[zk_provable]` macro analyzes standard Rust functions and automatically generates the corresponding ZK arithmetic circuit (e.g., for `halo2` or `plonk`).
* **Safety:** The compiler ensures that the Rust logic and the generated ZK circuit are perfectly synchronized, preventing the notorious "circuit mismatch" bugs in Web3.

### 3. Architecture
```text
[ Rust Application Logic (Probabilistic / ZK) ]
       │
       ▼
[ Mathematical IR Generator ] ──(Arithmetic Constraints)──> [ tpt-telos SMT ]
       │ (Verifies variance propagation & circuit equivalence)
       ▼
[ Optimized Probabilistic Code / ZK Circuit (R1CS/PLONK) ]
```

### 4. API Examples
```rust
use tpt_axiom::prelude::*;

// --- Probabilistic Types ---
// Sensor readings with inherent uncertainty
let pos_reading: Fuzzy<f64> = Fuzzy::new(mean: 10.5, variance: 0.5);
let vel_reading: Fuzzy<f64> = Fuzzy::new(mean: 2.0, variance: 0.1);

// Math automatically propagates the uncertainty!
// The compiler calculates the new mean and variance based on probability rules.
let next_pos = pos_reading + (vel_reading * dt); 

// --- Native Zero-Knowledge ---
// Write standard Rust logic
#[zk_provable(backend = "halo2")]
pub fn prove_balance_transfer(
    pub sender_balance: u64,
    pub receiver_balance: u64,
    secret amount: u64,
) {
    // The compiler automatically translates this into an arithmetic circuit.
    assert!(sender_balance >= amount);
    assert!(receiver_balance + amount == new_receiver_balance);
}

// Generates the proving key and verifying key automatically at build time.
```

### 5. Implementation Strategy
1. **Phase 1:** Implement the `Fuzzy` and `Distribution` types with operator overloading for automatic variance propagation.
2. **Phase 2:** Build the `#[zk_provable]` proc-macro to parse Rust AST and translate it into an intermediate arithmetic representation.
3. **Phase 3:** Integrate `tpt-telos` to formally verify that the generated ZK circuit perfectly matches the Rust logic's constraints.
4. **Phase 4:** Release backend adapters for popular ZK frameworks (`halo2`, `arkworks`, `sp1`).

### 6. Ecosystem Value
`tpt-axiom` opens up Rust to highly specialized, high-value domains. It makes Rust the default language for autonomous robotics (via native probabilistic types) and the most secure language for Web3/privacy-preserving applications (via native ZK primitives), creating a massive competitive moat.