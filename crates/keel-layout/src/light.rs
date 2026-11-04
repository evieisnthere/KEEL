//! Light colour: Stormworks lights take their colour from the additive paint (`ac`), so the colour also sets the
//! brightness (a darker colour is a dimmer light). KEEL sets it from a colour temperature in kelvin and a brightness.

/// Approximate sRGB of a black body at `kelvin` (1000–12000 K), normalised so the brightest channel is 1.
pub fn kelvin_rgb(kelvin: f64) -> [f64; 3] {
    let t = kelvin.clamp(1000.0, 12000.0) / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698727446 * (t - 60.0).powf(-0.1332047592) };
    let g = if t <= 66.0 { 99.4708025861 * t.ln() - 161.1195681661 } else { 288.1221695283 * (t - 60.0).powf(-0.0755148492) };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.5177312231 * (t - 10.0).ln() - 305.0447927307
    };
    let c = [r, g, b].map(|v: f64| v.clamp(0.0, 255.0));
    let m = c.iter().cloned().fold(1.0, f64::max);
    c.map(|v| v / m)
}

/// Light paint: the kelvin colour scaled by brightness (0–1).
pub fn light_colour(kelvin: f64, brightness: f64) -> String {
    let c = kelvin_rgb(kelvin).map(|v| v * brightness.clamp(0.0, 1.0));
    crate::build::hex(c)
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LightSetting {
    pub kelvin: f64,
    pub brightness: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_is_red_heavy_and_dim_is_darker() {
        let w = kelvin_rgb(2700.0);
        assert!(w[0] > w[1] && w[1] > w[2]);
        let d = kelvin_rgb(6500.0);
        assert!((d[0] - d[2]).abs() < 0.08, "{d:?}");
        assert_eq!(light_colour(6500.0, 1.0).len(), 6);
        assert!(light_colour(3000.0, 0.3) < light_colour(3000.0, 0.9));
    }
}
