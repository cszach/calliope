//! Page zoom steps, matching the levels browsers commonly use.

pub const STEPS: &[f64] = &[
    0.3, 0.5, 0.67, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

const EPSILON: f64 = 0.001;

/// The next step above `level`, or `level` itself at the top.
pub fn zoom_in(level: f64) -> f64 {
    STEPS
        .iter()
        .copied()
        .find(|s| *s > level + EPSILON)
        .unwrap_or(level)
}

/// The next step below `level`, or `level` itself at the bottom.
pub fn zoom_out(level: f64) -> f64 {
    STEPS
        .iter()
        .rev()
        .copied()
        .find(|s| *s < level - EPSILON)
        .unwrap_or(level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_up_and_down_from_default() {
        assert_eq!(zoom_in(1.0), 1.1);
        assert_eq!(zoom_out(1.0), 0.9);
    }

    #[test]
    fn off_step_levels_snap_to_the_next_step() {
        assert_eq!(zoom_in(1.2), 1.25);
        assert_eq!(zoom_out(1.2), 1.1);
    }

    #[test]
    fn stops_at_the_ends() {
        assert_eq!(zoom_in(3.0), 3.0);
        assert_eq!(zoom_out(0.3), 0.3);
    }

    #[test]
    fn float_noise_does_not_skip_a_step() {
        assert_eq!(zoom_in(1.1 + 1e-9), 1.25);
        assert_eq!(zoom_out(0.9 - 1e-9), 0.8);
    }
}
