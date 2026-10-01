//! A minimal 1-D Kalman filter built from ordinary arithmetic on `Fuzzy<f64>`.
//!
//! This is the Phase 1 milestone example from `todo.md`: sensor fusion using
//! only `+`/`*` on `Fuzzy<T>`, with variance propagated automatically.

use tpt_axiom::prelude::*;

/// Predict the next state from a constant-velocity motion model.
fn predict(position: Fuzzy<f64>, velocity: Fuzzy<f64>, dt: f64) -> Fuzzy<f64> {
    position + velocity * dt
}

/// Fuse a prediction with a noisy sensor measurement via inverse-variance
/// weighting (the standard 1-D Kalman update).
fn update(prediction: Fuzzy<f64>, measurement: Fuzzy<f64>) -> Fuzzy<f64> {
    let gain = prediction.variance() / (prediction.variance() + measurement.variance());
    let mean = gain.mul_add(measurement.mean() - prediction.mean(), prediction.mean());
    let variance = (1.0 - gain) * prediction.variance();
    Fuzzy::new(mean, variance)
}

fn main() {
    let velocity = Fuzzy::new(2.0, 0.1);
    let dt = 1.0;

    let measurements = [10.6, 12.4, 14.6, 16.5, 18.4];
    let mut estimate = Fuzzy::new(10.5, 0.5);

    println!("step  mean      variance");
    println!(
        "init  {:.3}     {:.3}",
        estimate.mean(),
        estimate.variance()
    );

    for (step, &measured_position) in measurements.iter().enumerate() {
        let prediction = predict(estimate, velocity, dt);
        let measurement = Fuzzy::new(measured_position, 0.8);
        estimate = update(prediction, measurement);
        println!(
            "{:>4}  {:.3}     {:.3}",
            step + 1,
            estimate.mean(),
            estimate.variance()
        );
    }
}
