//! Sensor fusion: three range sensors with different noise characteristics
//! observe the same distance. [`Fuzzy::fuse`] combines independent estimates
//! with the minimum-variance (Kalman-style) formula, and the fused estimate
//! is provably no more uncertain than any single sensor.
//!
//! ```text
//! cargo run -p tpt-axiom --example sensor_fusion
//! ```

use tpt_axiom::prelude::*;

fn main() {
    // Three sensors measuring the same 10 m distance:
    //   lidar  — sharp but slightly biased noise (σ = 0.10 m)
    //   radar  — noisy (σ = 0.50 m)
    //   sonar  — very noisy (σ = 1.00 m)
    let lidar = Fuzzy::new(10.02_f64, 0.10 * 0.10);
    let radar = Fuzzy::new(9.85_f64, 0.50 * 0.50);
    let sonar = Fuzzy::new(10.40_f64, 1.00 * 1.00);

    println!("independent estimates of the same 10 m distance:");
    for (name, sensor) in [("lidar", &lidar), ("radar", &radar), ("sonar", &sonar)] {
        println!(
            "  {name:5} mean = {:6.3} m, σ = {:.3} m",
            sensor.mean(),
            sensor.standard_deviation()
        );
    }

    // Pairwise-independent Gaussian estimates fuse in any order — the
    // minimum-variance combination is associative up to floating-point.
    let fused = lidar.fuse(&radar).fuse(&sonar);

    println!("\nfused estimate:");
    let (lo, hi) = fused.confidence_interval(0.95);
    println!(
        "  mean = {:.4} m, σ = {:.4} m (95% CI: {:.3} – {:.3} m)",
        fused.mean(),
        fused.standard_deviation(),
        lo,
        hi
    );

    // The fuse contract: no input can be more certain than the result.
    assert!(fused.variance() < lidar.variance());
    assert!(fused.variance() < radar.variance());
    assert!(fused.variance() < sonar.variance());
    // And the fused mean must sit inside the sensors' span.
    assert!(fused.mean() > radar.mean() && fused.mean() < sonar.mean());
    println!("\nfused σ is smaller than every individual sensor's σ.");
}
