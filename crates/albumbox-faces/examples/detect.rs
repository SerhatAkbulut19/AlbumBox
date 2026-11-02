//! Manual check: `cargo run -p albumbox-faces --example detect -- <models dir> <image> [<image> …]`
//! Prints the faces found and the similarity between every pair of faces.

use albumbox_faces::{cluster, FaceEngine, ModelFiles};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let files = ModelFiles::find(std::path::Path::new(&args[0])).ok_or_else(|| anyhow::anyhow!("models not found"))?;
    let engine = FaceEngine::load(&files, 4)?;
    let mut all = Vec::new();
    for path in &args[1..] {
        let img = image::open(path)?.to_rgb8();
        let t = std::time::Instant::now();
        let faces = engine.analyze(&img)?;
        println!("{path}: {} faces in {:?} ({}x{})", faces.len(), t.elapsed(), img.width(), img.height());
        for (i, f) in faces.iter().enumerate() {
            println!("  #{i} score {:.2} bbox {:?}", f.score, f.bbox.map(|v| (v * 1000.0).round() / 1000.0));
            let out = std::env::temp_dir().join(format!("albumbox-face-{}-{i}.jpg", all.len()));
            f.crop.save(&out)?;
            all.push((format!("{path}#{i}"), f.embedding.clone()));
        }
    }
    for i in 0..all.len() {
        for j in i + 1..all.len() {
            let s = cluster::similarity(&all[i].1, &all[j].1);
            if s > 0.3 {
                println!("similar {:.2}: {} ~ {}", s, all[i].0, all[j].0);
            }
        }
    }
    Ok(())
}
