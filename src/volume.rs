/// Convert the user-facing slider position to audio amplitude. Squaring gives
/// the quiet end more useful travel while retaining silence and full volume.
pub fn amplitude(volume: f32) -> f32 {
    if !volume.is_finite() {
        return 0.;
    }
    volume.clamp(0., 1.).powi(2)
}

#[cfg(test)]
mod tests {
    use super::amplitude;

    #[test]
    fn quiet_range_is_precise_and_the_entire_scale_is_monotonic() {
        assert_eq!(amplitude(0.), 0.);
        assert_eq!(amplitude(1.), 1.);
        assert!((amplitude(0.05) - 0.0025).abs() < 0.000001);
        assert!(amplitude(0.001) > 0.);
        for step in 1..=1000 {
            assert!(amplitude(step as f32 / 1000.) > amplitude((step - 1) as f32 / 1000.));
        }
        assert_eq!(amplitude(-1.), 0.);
        assert_eq!(amplitude(2.), 1.);
        assert_eq!(amplitude(f32::NAN), 0.);
    }
}
