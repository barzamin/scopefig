use std::{
    fs::{self, File},
    io::{BufWriter, Seek, Write},
    path::PathBuf,
};

use anyhow::{anyhow, Result};
use hound::WavWriter;
use kv_log_macro as log;
use lyon_geom::{LineSegment, Point};
use lyon_path::Path;
use lyon_path::{iterator::Flattened, PathEvent};
use structopt::StructOpt;
use usvg::{prelude::*, Align, FitTo, Size, Transform, ViewBox};

mod svg;

#[derive(Debug, StructOpt)]
struct Args {
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(short, long, parse(from_os_str), default_value = "out.wav")]
    output: PathBuf,
}

const F_s: u32 = 44_100; // Hz
const DRAW_VELOCITY: f32 = 50.; // units/s
                                  // const TRANSIT_VELOCITY: f32 = 40000.; // units/s
const TOLERANCE: f32 = 0.001;

fn draw_line(pts: &mut Vec<Point<f32>>, line: LineSegment<f32>) {
    log::debug!("emit line {:?}", line);

    let n_samples: usize = (F_s as f32 * line.length() / DRAW_VELOCITY).trunc() as usize;
    log::debug!("  drawing line with n_samples: {}", n_samples);
    for t in (0..n_samples).map(|i| i as f32 / n_samples as f32) {
        log::trace!("    generate pt t : {}, sample : {:?}", t, line.sample(t));
        pts.push(line.sample(t));
    }
}

fn write_wav<W>(wtr: W, pts: &[Point<f32>]) -> Result<(), hound::Error>
where
    W: Write + Seek,
{
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: F_s,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut wav_wtr = hound::WavWriter::new(wtr, spec)?;
    for _ in 0..10 {
        for pt in pts {
            wav_wtr.write_sample(pt.x)?;
            wav_wtr.write_sample(pt.y)?;
        }
    }
    wav_wtr.finalize()?;

    Ok(())
}

fn transform(
    base_txform: usvg::Transform,
    view_box: ViewBox,
    size: usvg::Size,
) -> lyon_geom::Transform<f32> {
    let scale = (2. / view_box.rect.width().max(view_box.rect.height())) as f32;
    log::debug!("scale: {}", scale);

    lyon_geom::Transform::<f32>::new(
        base_txform.a as f32,
        base_txform.b as f32,
        base_txform.c as f32,
        base_txform.d as f32,
        base_txform.e as f32,
        base_txform.f as f32,
    )
    .then_translate(lyon_geom::Vector::new(
        (-view_box.rect.width() / 2.) as f32,
        (-view_box.rect.height() / 2.) as f32,
    ))
    .then_scale(scale, scale)
}

fn main() -> Result<()> {
    femme::with_level(femme::LevelFilter::Trace);

    let args = Args::from_args();

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_file(args.input, &opt)?;

    let mut pts: Vec<Point<f32>> = vec![];

    let view_box = tree.svg_node().view_box;
    let size = tree.svg_node().size;
    log::info!("view_box: {:?}, size: {:?}", view_box, size);
    log::info!("{:?}", size.fit_view_box(&view_box));

    for node in tree.root().descendants() {
        if let usvg::NodeKind::Path(ref p) = *node.borrow() {
            log::debug!("handling node {:?}", node);
            let mut txform = transform(node.transform(), view_box, size);
            println!("txform: {:?}", txform);
            let flattened = Flattened::new(TOLERANCE, svg::convert_path(p));
            for evt in flattened {
                log::trace!(" -> {:?}", evt);
                match evt {
                    lyon_path::Event::Begin { at } => {
                        // slew at full speed to point
                        if let Some(last) = pts.last() {
                        } else {
                        }
                        pts.push(txform.transform_point(at));
                    }
                    lyon_path::Event::End { last, first, close } => {
                        if close {
                            let line = LineSegment {
                                from: txform.transform_point(last),
                                to: txform.transform_point(first),
                            };
                            draw_line(&mut pts, line);
                        }
                    }
                    lyon_path::Event::Line { from, to } => {
                        let line = LineSegment {
                            from: txform.transform_point(from),
                            to: txform.transform_point(to),
                        };
                        draw_line(&mut pts, line);
                    }
                    _ => {
                        log::warn!("unsupported path element {:?}", evt);
                    }
                }
            }
        }
    }

    write_wav(BufWriter::new(File::create(args.output)?), &pts)?;

    Ok(())
}
