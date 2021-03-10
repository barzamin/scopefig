use core::f32::consts::PI;

use anyhow::Result;

fn main() -> Result<()> {
    let out_spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create("sin", out_spec)?;

    for t in (0..44100).map(|x| x as f32 / 44100.0) {
        let y = (t * 440. * 2. * PI).sin();
        let x = (t * 440. * 2. * PI).cos();
        writer.write_sample(x)?;
        writer.write_sample(y)?;
    }

    writer.finalize()?;

    Ok(())
}
